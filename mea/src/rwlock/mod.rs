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

//! A reader-writer lock that allows multiple readers or a single writer at a time.
//!
//! This type of lock allows a number of readers or at most one writer at any point in time. The
//! write portion of this lock typically allows modification of the underlying data (exclusive
//! access) and the read portion of this lock typically allows for read-only access (shared access).
//!
//! In comparison, a [`Mutex`] does not distinguish between readers or writers that acquire the
//! lock, therefore causing any tasks waiting for the lock to become available to yield. An RwLock
//! will allow any number of readers to acquire the lock as long as a writer is not holding the
//! lock.
//!
//! The priority policy of Tokio's read-write lock is fair (or [write-preferring]), in order to
//! ensure that readers cannot starve writers. Fairness is ensured using a first-in, first-out queue
//! for the tasks awaiting the lock; if a task that wishes to acquire the write lock is at the head
//! of the queue, read locks will not be given out until the write lock has been released. This is
//! in contrast to the Rust standard library's `std::sync::RwLock`, where the priority policy is
//! dependent on the operating system's implementation.
//!
//! The type parameter `T` represents the data that this lock protects. It is required that `T`
//! satisfies [`Send`] to be shared across threads. The RAII guards returned from the locking
//! methods implement [`Deref`] (and [`DerefMut`] for the `write` method) to allow access to the
//! content of the lock.
//!
//! # Examples
//!
//! ```
//! # #[tokio::main]
//! # async fn main() {
//! use mea::rwlock::RwLock;
//!
//! let lock = RwLock::new(5);
//!
//! // many reader locks can be held at once
//! {
//!     let r1 = lock.read().await;
//!     let r2 = lock.read().await;
//!     assert_eq!(*r1, 5);
//!     assert_eq!(*r2, 5);
//! } // read locks are dropped at this point
//!
//! // only one write lock may be held, however
//! {
//!     let mut w = lock.write().await;
//!     *w += 1;
//!     assert_eq!(*w, 6);
//! } // write lock is dropped here
//! # }
//! ```
//!
//! [`Mutex`]: crate::mutex::Mutex
//! [`Deref`]: std::ops::Deref
//! [`DerefMut`]: std::ops::DerefMut
//! [write-preferring]: https://en.wikipedia.org/wiki/Readers%E2%80%93writer_lock#Priority_policies

use std::cell::UnsafeCell;
use std::fmt;
use std::sync::Arc;

use crate::internal::Semaphore;

mod mapped_read_guard;
pub use mapped_read_guard::MappedRwLockReadGuard;
mod mapped_write_guard;
pub use mapped_write_guard::MappedRwLockWriteGuard;
mod owned_mapped_read_guard;
pub use owned_mapped_read_guard::OwnedMappedRwLockReadGuard;
mod owned_mapped_write_guard;
pub use owned_mapped_write_guard::OwnedMappedRwLockWriteGuard;
mod owned_read_guard;
pub use owned_read_guard::OwnedRwLockReadGuard;
mod owned_write_guard;
pub use owned_write_guard::OwnedRwLockWriteGuard;
mod read_guard;
pub use read_guard::RwLockReadGuard;
mod write_guard;
pub use write_guard::RwLockWriteGuard;

#[cfg(test)]
mod test;

/// A reader-writer lock that allows multiple readers or a single writer at a time.
///
/// See the [module level documentation](self) for more.
pub struct RwLock<T: ?Sized> {
    /// Maximum number of concurrent readers.
    max_readers: usize,
    /// Semaphore to coordinate read and write access to T
    s: Semaphore,
    /// The inner data.
    c: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

impl<T> From<T> for RwLock<T> {
    fn from(t: T) -> Self {
        Self::new(t)
    }
}

impl<T: Default> Default for RwLock<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("RwLock");
        match self.try_read() {
            Some(inner) => d.field("data", &&*inner),
            None => d.field("data", &format_args!("<locked>")),
        };
        d.finish()
    }
}

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
        let default_max_readers = usize::MAX >> 1;
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
    pub fn with_max_readers(t: T, max_readers: usize) -> RwLock<T> {
        let s = Semaphore::new(max_readers);
        let c = UnsafeCell::new(t);
        RwLock { max_readers, c, s }
    }

    /// Consumes the lock, returning the underlying data.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::rwlock::RwLock;
    ///
    /// let lock = RwLock::new(1);
    /// let n = lock.into_inner();
    /// assert_eq!(n, 1);
    /// ```
    pub fn into_inner(self) -> T {
        self.c.into_inner()
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `RwLock` mutably, no actual locking needs to take place: the
    /// mutable borrow statically guarantees no locks exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::rwlock::RwLock;
    ///
    /// let mut lock = RwLock::new(1);
    /// let n = lock.get_mut();
    /// *n = 2;
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        self.c.get_mut()
    }
}


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

impl<T: ?Sized> RwLock<T> {
    /// Locks this `RwLock` with shared read access, causing the current task to yield until the
    /// lock has been acquired.
    ///
    /// The calling task will yield until there are no writers which hold the lock. There may be
    /// other readers inside the lock when the task resumes.
    ///
    /// This method is identical to [`RwLock::read`], except that the returned guard references the
    /// `RwLock` with an [`Arc`] rather than by borrowing it. Therefore, the `RwLock` must be
    /// wrapped in an `Arc` to call this method, and the guard will live for the `'static` lifetime,
    /// as it keeps the `RwLock` alive by holding an `Arc`.
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
    /// Cancelling a call to `read_owned` makes you lose your place in the queue.
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
    /// let n = lock.read_owned().await;
    /// assert_eq!(*n, 1);
    ///
    /// tokio::spawn(async move {
    ///     // while the outer read lock is held, we acquire a read lock, too
    ///     let r = lock_clone.read_owned().await;
    ///     assert_eq!(*r, 1);
    /// })
    /// .await
    /// .unwrap();
    /// # }
    /// ```
    pub async fn read_owned(self: Arc<Self>) -> OwnedRwLockReadGuard<T> {
        self.s.acquire(1).await;
        OwnedRwLockReadGuard { lock: self }
    }

    /// Attempts to acquire this `RwLock` with shared read access.
    ///
    /// If the access couldn't be acquired immediately, returns `None`. Otherwise, an RAII guard is
    /// returned which will release read access when dropped.
    ///
    /// This method is identical to [`RwLock::try_read`], except that the returned guard references
    /// the `RwLock` with an [`Arc`] rather than by borrowing it. Therefore, the `RwLock` must
    /// be wrapped in an `Arc` to call this method, and the guard will live for the `'static`
    /// lifetime, as it keeps the `RwLock` alive by holding an `Arc`.
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
    /// let v = lock.clone().try_read_owned().unwrap();
    /// assert_eq!(*v, 1);
    /// drop(v);
    ///
    /// let v = lock.try_write().unwrap();
    /// assert!(lock.clone().try_read_owned().is_none());
    /// ```
    pub fn try_read_owned(self: Arc<Self>) -> Option<OwnedRwLockReadGuard<T>> {
        if self.s.try_acquire(1) {
            Some(OwnedRwLockReadGuard { lock: self })
        } else {
            None
        }
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Locks this `RwLock` with exclusive write access, causing the current task to yield until the
    /// lock has been acquired.
    ///
    /// The calling task will yield while other writers or readers currently have access to the
    /// lock.
    ///
    /// This method is identical to [`RwLock::write`], except that the returned guard references the
    /// `RwLock` with an [`Arc`] rather than by borrowing it. Therefore, the `RwLock` must be
    /// wrapped in an `Arc` to call this method, and the guard will live for the `'static` lifetime,
    /// as it keeps the `RwLock` alive by holding an `Arc`.
    ///
    /// Returns an RAII guard which will drop the write access of this `RwLock` when dropped.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute locks in the order they were requested.
    /// Cancelling a call to `write_owned` makes you lose your place in the queue.
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
    /// let mut n = lock.write_owned().await;
    /// *n = 2;
    /// # }
    /// ```
    pub async fn write_owned(self: Arc<Self>) -> OwnedRwLockWriteGuard<T> {
        self.s.acquire(self.max_readers).await;
        OwnedRwLockWriteGuard {
            permits_acquired: self.max_readers,
            lock: self,
        }
    }

    /// Attempts to acquire this `RwLock` with exclusive write access.
    ///
    /// If the access couldn't be acquired immediately, returns `None`. Otherwise, an RAII guard is
    /// returned which will release write access when dropped.
    ///
    /// This method is identical to [`RwLock::try_write`], except that the returned guard references
    /// the `RwLock` with an [`Arc`] rather than by borrowing it. Therefore, the `RwLock` must
    /// be wrapped in an `Arc` to call this method, and the guard will live for the `'static`
    /// lifetime, as it keeps the `RwLock` alive by holding an `Arc`.
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
    /// assert!(lock.clone().try_write_owned().is_none());
    /// drop(v);
    ///
    /// let mut v = lock.try_write_owned().unwrap();
    /// *v = 2;
    /// ```
    pub fn try_write_owned(self: Arc<Self>) -> Option<OwnedRwLockWriteGuard<T>> {
        if self.s.try_acquire(self.max_readers) {
            Some(OwnedRwLockWriteGuard {
                permits_acquired: self.max_readers,
                lock: self,
            })
        } else {
            None
        }
    }
}
