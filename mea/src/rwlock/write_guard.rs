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
use std::ops::DerefMut;
use std::ptr::NonNull;

use crate::rwlock::MappedRwLockWriteGuard;
use crate::rwlock::RwLock;
use crate::rwlock::RwLockReadGuard;

impl<T: ?Sized> RwLock<T> {
    /// Locks this `RwLock` with exclusive write access, causing the current task to yield until the
    /// lock has been acquired.
    ///
    /// The calling task will yield while other writers or readers currently have access to the
    /// lock.
    ///
    /// Returns an RAII guard which will drop the write access of this `RwLock` when dropped.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute locks in the order they were requested.
    /// Cancelling a call to `write` makes you lose your place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::RwLock;
    ///
    /// let lock = RwLock::new(1);
    /// let mut n = lock.write().await;
    /// *n = 2;
    /// # }
    /// ```
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.s.acquire(self.max_readers).await;
        RwLockWriteGuard {
            permits_acquired: self.max_readers,
            lock: self,
        }
    }

    /// Attempts to acquire this `RwLock` with exclusive write access.
    ///
    /// If the access couldn't be acquired immediately, returns `None`. Otherwise, an RAII guard is
    /// returned which will release write access when dropped.
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
    /// assert!(lock.try_write().is_none());
    /// drop(v);
    ///
    /// let mut v = lock.try_write().unwrap();
    /// *v = 2;
    /// ```
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        if self.s.try_acquire(self.max_readers) {
            Some(RwLockWriteGuard {
                permits_acquired: self.max_readers,
                lock: self,
            })
        } else {
            None
        }
    }
}

/// RAII structure used to release the exclusive write access of a lock when dropped.
///
/// This structure is created by the [`RwLock::write`] method.
///
/// See the [module level documentation](crate::rwlock) for more.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    pub(super) permits_acquired: usize,
    pub(super) lock: &'a RwLock<T>,
}

unsafe impl<T: ?Sized + Send + Sync> Send for RwLockWriteGuard<'_, T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLockWriteGuard<'_, T> {}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.s.release(self.permits_acquired);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for RwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}

impl<'a, T: ?Sized> RwLockWriteGuard<'a, T> {
    /// Makes a new [`MappedRwLockWriteGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `RwLockWriteGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `RwLockWriteGuard::map(...)`.
    ///
    /// A method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::RwLock;
    /// use mea::rwlock::RwLockWriteGuard;
    ///
    /// #[derive(Debug)]
    /// struct Foo {
    ///     a: u32,
    ///     b: String,
    /// }
    ///
    /// let rwlock = RwLock::new(Foo {
    ///     a: 1,
    ///     b: "hello".to_owned(),
    /// });
    ///
    /// let mut guard = rwlock.write().await;
    /// let mut mapped_guard = RwLockWriteGuard::map(guard, |foo| &mut foo.a);
    ///
    /// *mapped_guard = 42;
    /// assert_eq!(*mapped_guard, 42);
    /// # }
    /// ```
    pub fn map<U, F>(orig: Self, f: F) -> MappedRwLockWriteGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
        U: ?Sized,
    {
        let d = NonNull::from(f(unsafe { &mut *orig.lock.c.get() }));
        let permits_acquired = orig.permits_acquired;
        let orig = ManuallyDrop::new(orig);
        MappedRwLockWriteGuard::new(d, &orig.lock.s, permits_acquired)
    }

    /// Attempts to make a new [`MappedRwLockWriteGuard`] for a component of the
    /// locked data. The original guard is returned if the closure returns `None`.
    ///
    /// This operation cannot fail as the `RwLockWriteGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `RwLockWriteGuard::filter_map(...)`.
    ///
    /// A method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::RwLock;
    /// use mea::rwlock::RwLockWriteGuard;
    ///
    /// #[derive(Debug)]
    /// struct Foo {
    ///     a: u32,
    ///     b: String,
    /// }
    ///
    /// let rwlock = RwLock::new(Foo {
    ///     a: 11,
    ///     b: "ok".to_owned(),
    /// });
    ///
    /// let mut guard = rwlock.write().await;
    /// let mut mapped_guard =
    ///     RwLockWriteGuard::filter_map(
    ///         guard,
    ///         |foo| {
    ///             if foo.a > 10 {
    ///                 Some(&mut foo.a)
    ///             } else {
    ///                 None
    ///             }
    ///         },
    ///     )
    ///     .expect("should have mapped");
    ///
    /// *mapped_guard = 12;
    /// assert_eq!(*mapped_guard, 12);
    /// # }
    /// ```
    pub fn filter_map<U, F>(orig: Self, f: F) -> Result<MappedRwLockWriteGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
        U: ?Sized,
    {
        match f(unsafe { &mut *orig.lock.c.get() }) {
            Some(d) => {
                let d = NonNull::from(d);
                let permits_acquired = orig.permits_acquired;
                let orig = ManuallyDrop::new(orig);
                Ok(MappedRwLockWriteGuard::new(
                    d,
                    &orig.lock.s,
                    permits_acquired,
                ))
            }
            None => Err(orig),
        }
    }

    /// Atomically downgrades the write lock to a read lock.
    ///
    /// This method changes the lock from exclusive mode to shared mode atomically,
    /// preventing other writers from acquiring the lock in between.
    ///
    /// This is more efficient than dropping the write guard and acquiring a new read guard.
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
    ///
    /// let mut write_guard = lock.write().await;
    /// *write_guard = 2;
    ///
    /// let read_guard = write_guard.downgrade();
    /// assert_eq!(*read_guard, 2);
    ///
    /// assert!(lock.try_write().is_none());
    ///
    /// drop(read_guard);
    /// assert!(lock.try_write().is_some());
    /// # }
    /// ```
    pub fn downgrade(self) -> RwLockReadGuard<'a, T> {
        // Prevent the original write guard from running its Drop implementation,
        // which would release all permits. This must be done BEFORE any operation
        // that might panic to ensure panic safety.
        let guard = ManuallyDrop::new(self);

        // Release max_readers - 1 permits to convert the write lock to a read lock.
        // The remaining 1 permit is kept for the read lock.
        guard.lock.s.release(guard.permits_acquired - 1);
        RwLockReadGuard { lock: guard.lock }
    }
}
