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

//! An asynchronous reader-writer lock.

use std::cell::UnsafeCell;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::internal::Semaphore;

/// An asynchronous reader-writer lock.
pub struct RwLock<T: ?Sized> {
    /// Maximum number of concurrent readers.
    max_readers: u32,
    /// Semaphore to coordinate read and write access to T
    s: Semaphore,
    /// The inner data.
    c: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    /// Creates a new reader-writer lock in an unlocked state ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::rwlock::RwLock;
    ///
    /// let rwlock = RwLock::new(5);
    /// ```
    pub fn new(t: T) -> RwLock<T> {
        // large enough while not touch the edge
        let default_max_readers = u32::MAX >> 1;
        RwLock::with_max_readers(t, default_max_readers)
    }

    /// Creates a new reader-writer lock in an unlocked state, and allows a maximum of
    /// `max_readers` concurrent readers.
    ///
    /// This method is typically used for debugging and testing purposes.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::rwlock::RwLock;
    ///
    /// let rwlock = RwLock::with_max_readers(5, 1024);
    /// ```
    pub fn with_max_readers(t: T, max_readers: u32) -> RwLock<T> {
        let s = Semaphore::new(max_readers);
        let c = UnsafeCell::new(t);
        RwLock { max_readers, c, s }
    }

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
        RwLockReadGuard {
            s: &self.s,
            data: self.c.get(),
        }
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
            Some(RwLockReadGuard {
                s: &self.s,
                data: self.c.get(),
            })
        } else {
            None
        }
    }

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
            s: &self.s,
            data: self.c.get(),
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
                s: &self.s,
                data: self.c.get(),
            })
        } else {
            None
        }
    }
}

/// RAII structure used to release the shared read access of a lock when dropped.
///
/// This structure is created by the [`read`] method on [`RwLock`].
///
/// [`read`]: RwLock::read
/// [`RwLock`]: RwLock
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    s: &'a Semaphore,
    data: *const T,
}

unsafe impl<T> Send for RwLockReadGuard<'_, T> where T: ?Sized + Sync {}
unsafe impl<T> Sync for RwLockReadGuard<'_, T> where T: ?Sized + Send + Sync {}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.s.release(1);
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
        unsafe { &*self.data }
    }
}

/// RAII structure used to release the exclusive write access of a lock when dropped.
///
/// This structure is created by the [`write`] method on [`RwLock`].
///
/// [`write`]: RwLock::write
/// [`RwLock`]: RwLock
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    permits_acquired: u32,
    s: &'a Semaphore,
    data: *mut T,
}

unsafe impl<T> Send for RwLockWriteGuard<'_, T> where T: ?Sized + Send + Sync {}
unsafe impl<T> Sync for RwLockWriteGuard<'_, T> where T: ?Sized + Send + Sync {}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.s.release(self.permits_acquired);
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
        unsafe { &*self.data }
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.data }
    }
}
