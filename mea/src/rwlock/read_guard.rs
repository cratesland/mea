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

use crate::rwlock::RwLock;

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
