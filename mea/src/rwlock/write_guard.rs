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

use crate::rwlock::RwLock;

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
