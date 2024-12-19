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

//! An async mutex for protecting shared data.
//!
//! Unlike a standard mutex, this implementation is designed to work with async/await,
//! ensuring tasks yield properly when the lock is contended. This makes it suitable
//! for protecting shared resources in async code.
//!
//! This mutex will block tasks waiting for the lock to become available. The
//! mutex can be created via [`new`] and the protected data can be accessed
//! via the async [`lock`] method.
//!
//! # Examples
//!
//! ```
//! # #[tokio::main]
//! # async fn main() {
//! use std::sync::Arc;
//!
//! use mea::mutex::Mutex;
//!
//! let mutex = Arc::new(Mutex::new(0));
//! let mut handles = Vec::new();
//!
//! for i in 0..3 {
//!     let mutex = mutex.clone();
//!     handles.push(tokio::spawn(async move {
//!         let mut lock = mutex.lock().await;
//!         *lock += i;
//!     }));
//! }
//!
//! for handle in handles {
//!     handle.await.unwrap();
//! }
//!
//! let final_value = mutex.lock().await;
//! assert_eq!(*final_value, 3); // 0 + 1 + 2
//! #  }
//! ```
//!
//! [`new`]: Mutex::new
//! [`lock`]: Mutex::lock

use std::cell::UnsafeCell;
use std::fmt;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::primitives::internal;

/// An async mutex for protecting shared data.
///
/// See the [module level documentation](self) for more.
#[derive(Debug)]
pub struct Mutex<T: ?Sized> {
    s: internal::Semaphore,
    c: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    /// Creates a new mutex in an unlocked state ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let mutex = Mutex::new(5);
    /// ```
    pub fn new(t: T) -> Self {
        let s = internal::Semaphore::new(1);
        let c = UnsafeCell::new(t);
        Self { s, c }
    }

    /// Consumes the mutex, returning the underlying data.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let mutex = Mutex::new(1);
    /// let n = mutex.into_inner();
    /// assert_eq!(n, 1);
    /// ```
    pub fn into_inner(self) -> T {
        self.c.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Locks this mutex, causing the current task to yield until the lock has been acquired. When
    /// the lock has been acquired, function returns a [`MutexGuard`].
    ///
    /// This method is async and will yield the current task if the mutex is currently held by
    /// another task. When the mutex becomes available, the task will be woken up and given the
    /// lock.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute locks in the order they were requested.
    /// Cancelling a call to `lock` makes you lose your place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mutex::Mutex;
    /// let mutex = Mutex::new(1);
    ///
    /// let mut n = mutex.lock().await;
    /// *n = 2;
    /// # }
    /// ```
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        self.s.acquire(1).await;
        MutexGuard { lock: self }
    }

    /// Attempts to acquire the lock, and returns `None` if the lock is currently held somewhere
    /// else.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let mutex = Mutex::new(1);
    /// let guard = mutex.try_lock();
    /// match guard {
    ///     Some(mut guard) => {
    ///         *guard += 1;
    ///     }
    ///     None => {
    ///         println!("mutex is locked");
    ///     }
    /// }
    /// ```
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.s.try_acquire(1) {
            let guard = MutexGuard { lock: self };
            Some(guard)
        } else {
            None
        }
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `Mutex` mutably, no actual locking needs to
    /// take place -- the mutable borrow statically guarantees no locks exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let mut mutex = Mutex::new(1);
    /// let n = mutex.get_mut();
    /// *n = 2;
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        self.c.get_mut()
    }
}

/// RAII structure used to release the exclusive lock on a mutex when dropped.
///
/// This structure is created by the [`lock`] and [`try_lock`] methods on [`Mutex`].
///
/// [`lock`]: Mutex::lock
/// [`try_lock`]: Mutex::try_lock
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}

pub(crate) fn guard_lock<'a, T: ?Sized>(guard: &MutexGuard<'a, T>) -> &'a Mutex<T> {
    guard.lock
}

unsafe impl<T> Sync for MutexGuard<'_, T> where T: ?Sized + Send + Sync {}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.s.release(1);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}
