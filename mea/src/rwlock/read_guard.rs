// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::NonNull;

use crate::rwlock::MappedRwLockReadGuard;
use crate::rwlock::RwLock;
use crate::rwlock::RwLockWriteGuard;

impl<T: ?Sized> RwLock<T> {
    /// Locks this `RwLock` with shared read access, causing the current task to yield until the
    /// lock has been acquired.
    ///
    /// The calling task will yield until there are no writers which hold the lock. There may be
    /// other readers inside the lock when the task resumes.
    ///
    /// Note that under the priority policy of [`RwLock`], read locks are not granted until prior
    /// write locks, to prevent starvation. Therefore, deadlock may occur if a read lock is held
    /// by the current task, a write lock attempt is made, and then a subsequent read lock attempt
    /// is made by the current task.
    ///
    /// Returns an RAII guard which will drop this read access of the `RwLock` when dropped.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute locks in the order they were requested.
    /// Cancelling a call to `read` makes you lose your place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use std::sync::Arc;
    ///
    /// use mea::rwlock::RwLock;
    ///
    /// let lock = Arc::new(RwLock::new(1));
    /// let lock_clone = lock.clone();
    ///
    /// let n = lock.read().await;
    /// assert_eq!(*n, 1);
    ///
    /// tokio::spawn(async move {
    ///     // while the outer read lock is held, we acquire a read lock, too
    ///     let r = lock_clone.read().await;
    ///     assert_eq!(*r, 1);
    /// })
    /// .await
    /// .unwrap();
    /// # }
    /// ```
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        self.s.acquire(1).await;
        RwLockReadGuard { lock: self }
    }

    /// Attempts to acquire this `RwLock` with shared read access.
    ///
    /// If the access couldn't be acquired immediately, returns `None`. Otherwise, an RAII guard is
    /// returned which will release read access when dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use mea::rwlock::RwLock;
    ///
    /// let lock = Arc::new(RwLock::new(1));
    ///
    /// let v = lock.try_read().unwrap();
    /// assert_eq!(*v, 1);
    /// drop(v);
    ///
    /// let v = lock.try_write().unwrap();
    /// assert!(lock.try_read().is_none());
    /// ```
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        if self.s.try_acquire(1) {
            Some(RwLockReadGuard { lock: self })
        } else {
            None
        }
    }
}

/// RAII structure used to release the shared read access of a lock when dropped.
///
/// This structure is created by the [`RwLock::read`] method.
///
/// See the [module level documentation](crate::rwlock) for more.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    lock: &'a RwLock<T>,
}

unsafe impl<T: ?Sized + Sync> Send for RwLockReadGuard<'_, T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockReadGuard<'_, T> {}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.s.release(1);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<'a, T: ?Sized> RwLockReadGuard<'a, T> {
    /// Creates a new `RwLockReadGuard` by consuming a `RwLockWriteGuard`
    /// during a downgrade operation.
    ///
    /// # Safety
    /// This is the only safe way to construct a `RwLockReadGuard` without
    /// acquiring a new semaphore permit, as it relies on the permit
    /// already held by the `RwLockWriteGuard`.
    pub(super) fn from_write_downgrade(write_guard: ManuallyDrop<RwLockWriteGuard<'a, T>>) -> Self {
        Self {
            lock: write_guard.lock,
        }
    }
    /// Makes a new [`MappedRwLockReadGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `RwLockReadGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::map(...)`.
    ///
    /// A method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::RwLock;
    /// use mea::rwlock::RwLockReadGuard;
    ///
    /// #[derive(Debug, Clone)]
    /// struct Foo(String);
    ///
    /// let rwlock = RwLock::new(Foo("hello".to_owned()));
    ///
    /// let guard = rwlock.read().await;
    /// let mapped_guard = RwLockReadGuard::map(guard, |f| &f.0);
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn map<U, F>(orig: Self, f: F) -> MappedRwLockReadGuard<'a, U>
    where
        F: FnOnce(&T) -> &U,
        U: ?Sized,
    {
        let d = NonNull::from(f(&*orig));
        let orig = ManuallyDrop::new(orig);
        MappedRwLockReadGuard::new(d, &orig.lock.s)
    }

    /// Attempts to make a new [`MappedRwLockReadGuard`] for a component of the
    /// locked data. The original guard is returned if the closure returns `None`.
    ///
    /// This operation cannot fail as the `RwLockReadGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::filter_map(...)`.
    ///
    /// A method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::RwLock;
    /// use mea::rwlock::RwLockReadGuard;
    ///
    /// #[derive(Debug, Clone)]
    /// struct Foo(String);
    ///
    /// let rwlock = RwLock::new(Foo("hello".to_owned()));
    ///
    /// let guard = rwlock.read().await;
    /// let mapped_guard =
    ///     RwLockReadGuard::filter_map(guard, |f| if f.0.len() > 3 { Some(&f.0) } else { None })
    ///         .expect("should have mapped");
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn filter_map<U, F>(orig: Self, f: F) -> Result<MappedRwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
        U: ?Sized,
    {
        match f(&*orig) {
            Some(d) => {
                let d = NonNull::from(d);
                let orig = ManuallyDrop::new(orig);
                Ok(MappedRwLockReadGuard::new(d, &orig.lock.s))
            }
            None => Err(orig),
        }
    }
}
