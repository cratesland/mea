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
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;

use crate::rwlock::RwLock;

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

/// Owned RAII structure used to release the exclusive write access of a lock when dropped.
///
/// This structure is created by the [`RwLock::write`] method.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct OwnedRwLockWriteGuard<T: ?Sized> {
    pub(super) permits_acquired: usize,
    pub(super) lock: Arc<RwLock<T>>,
}

unsafe impl<T: ?Sized + Send + Sync> Send for OwnedRwLockWriteGuard<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for OwnedRwLockWriteGuard<T> {}

impl<T: ?Sized> Drop for OwnedRwLockWriteGuard<T> {
    fn drop(&mut self) {
        self.lock.s.release(self.permits_acquired);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for OwnedRwLockWriteGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for OwnedRwLockWriteGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for OwnedRwLockWriteGuard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T: ?Sized> DerefMut for OwnedRwLockWriteGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}
