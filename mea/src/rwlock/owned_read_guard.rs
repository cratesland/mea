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
use std::sync::Arc;

use crate::rwlock::RwLock;

/// Owned RAII structure used to release the shared read access of a lock when dropped.
///
/// This structure is created by the [`RwLock::read`] method.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct OwnedRwLockReadGuard<T: ?Sized> {
    pub(super) lock: Arc<RwLock<T>>,
}

unsafe impl<T: ?Sized + Sync> Send for OwnedRwLockReadGuard<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for OwnedRwLockReadGuard<T> {}

impl<T: ?Sized> Drop for OwnedRwLockReadGuard<T> {
    fn drop(&mut self) {
        self.lock.s.release(1);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for OwnedRwLockReadGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for OwnedRwLockReadGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for OwnedRwLockReadGuard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}


impl<T: ?Sized> OwnedRwLockReadGuard<T> {
    /// Makes a new [`crate::rwlock::OwnedMappedRwLockReadGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `OwnedRwLockReadGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `OwnedRwLockReadGuard::map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use std::sync::Arc;
    /// use mea::rwlock::{RwLock, OwnedRwLockReadGuard};
    ///
    /// #[derive(Debug)]
    /// struct Foo {
    ///     a: u32,
    ///     b: String,
    /// }
    ///
    /// let rwlock = Arc::new(RwLock::new(Foo {
    ///     a: 1,
    ///     b: "hello".to_owned(),
    /// }));
    ///
    /// let guard = rwlock.read_owned().await;
    /// let mapped_guard = OwnedRwLockReadGuard::map(guard, |foo| &foo.a);
    ///
    /// assert_eq!(*mapped_guard, 1);
    /// # }
    /// ```
    pub fn map<U, F>(orig: Self, f: F) -> crate::rwlock::OwnedMappedRwLockReadGuard<T, U>
    where
        F: FnOnce(&T) -> &U,
        U: ?Sized,
    {
        // SAFETY: orig.lock.c.get() is a valid pointer to T that was created when the lock was acquired.
        // The guard guarantees shared access to the data through the rwlock, so dereferencing is safe.
        let d = std::ptr::NonNull::from(f(unsafe { &*orig.lock.c.get() }));
        let orig = std::mem::ManuallyDrop::new(orig);

        // Safely extract the Arc from the guard
        let lock = unsafe { std::ptr::read(&orig.lock) };

        crate::rwlock::OwnedMappedRwLockReadGuard::new(d, lock)
    }

    /// Attempts to make a new [`crate::rwlock::OwnedMappedRwLockReadGuard`] for a component of the locked data. The
    /// original guard is returned if the closure returns `None`.
    ///
    /// This operation cannot fail as the `OwnedRwLockReadGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `OwnedRwLockReadGuard::try_map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use std::sync::Arc;
    /// use mea::rwlock::{RwLock, OwnedRwLockReadGuard};
    ///
    /// #[derive(Debug)]
    /// struct Foo {
    ///     a: u32,
    ///     b: String,
    /// }
    ///
    /// let rwlock = Arc::new(RwLock::new(Foo {
    ///     a: 1,
    ///     b: "hello".to_owned(),
    /// }));
    ///
    /// let guard = rwlock.read_owned().await;
    /// let mapped_guard = OwnedRwLockReadGuard::try_map(guard, |foo| {
    ///     if foo.a > 0 {
    ///         Some(&foo.b)
    ///     } else {
    ///         None
    ///     }
    /// }).expect("should have mapped");
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn try_map<U, F>(orig: Self, f: F) -> Result<crate::rwlock::OwnedMappedRwLockReadGuard<T, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
        U: ?Sized,
    {
        // SAFETY: orig.lock.c.get() is a valid pointer to T that was created when the lock was acquired.
        // The guard guarantees shared access to the data through the rwlock, so dereferencing is safe.
        match f(unsafe { &*orig.lock.c.get() }) {
            Some(d) => {
                let d = std::ptr::NonNull::from(d);
                let orig = std::mem::ManuallyDrop::new(orig);

                // Safely extract the Arc from the guard
                let lock = unsafe { std::ptr::read(&orig.lock) };

                Ok(crate::rwlock::OwnedMappedRwLockReadGuard::new(d, lock))
            }
            None => Err(orig),
        }
    }
}