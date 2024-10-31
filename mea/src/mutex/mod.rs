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

use core::cell::UnsafeCell;
use core::fmt;
use core::ops::Deref;
use core::ops::DerefMut;

use crate::semaphore::Semaphore;

pub struct Mutex<T: ?Sized> {
    s: Semaphore,
    c: UnsafeCell<T>,
}

impl<T> From<T> for Mutex<T> {
    fn from(s: T) -> Self {
        Self::new(s)
    }
}

impl<T> Default for Mutex<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Mutex");
        match self.try_lock() {
            Some(data) => d.field("data", &&*data),
            None => d.field("data", &"<locked>"),
        };
        d.finish()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Creates a new lock in an unlocked state ready for use.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::mutex::Mutex;
    ///
    /// let lock = Mutex::new(5);
    /// ```
    pub const fn new(t: T) -> Self
    where
        T: Sized,
    {
        let c = UnsafeCell::new(t);
        let s = Semaphore::new(1);
        Self { c, s }
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
        if self.s.intern_try_acquire(1) {
            Some(MutexGuard { lock: self })
        } else {
            None
        }
    }

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
        self.s.intern_acquire(1).await;
        MutexGuard { lock: self }
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
    pub fn into_inner(self) -> T
    where
        T: Sized,
    {
        self.c.into_inner()
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
