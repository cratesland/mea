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

use std::cell::UnsafeCell;
use std::ops::Deref;
use std::ops::DerefMut;

use crate::internal;

pub struct Mutex<T: ?Sized> {
    s: internal::Semaphore,
    c: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    /// Creates a new lock in an unlocked state ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let lock = Mutex::new(5);
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
    /// Locks this mutex, causing the current task to yield until the lock has
    /// been acquired.  When the lock has been acquired, function returns a
    /// [`MutexGuard`].
    ///
    /// If the mutex is available to be acquired immediately, then this call
    /// will typically not yield to the runtime. However, this is not guaranteed
    /// under all circumstances.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    /// use pollster::FutureExt;
    ///
    /// let mutex = Mutex::new(1);
    /// let mut n = mutex.lock().block_on();
    /// *n = 2;
    /// assert_eq!(*n, 2);
    /// ```
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        let fut = async {
            self.s.acquire(1).await;
            MutexGuard { lock: self }
        };
        fut.await
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
    /// let n = mutex.try_lock().unwrap();
    /// assert_eq!(*n, 1);
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
    /// take place, i.e., the mutable borrow statically guarantees no locks exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let mut mutex = Mutex::new(1);
    /// let mut n = mutex.get_mut();
    /// *n = 2;
    /// assert_eq!(*n, 2);
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        self.c.get_mut()
    }
}

#[must_use = "if unused the Mutex will immediately unlock"]
pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.s.release(1);
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
