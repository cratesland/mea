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
use std::ptr::NonNull;

use crate::rwlock::{RwLock, MappedRwLockReadGuard};
/// RAII structure used to release the shared read access of a lock when dropped.
///
/// This structure is created by the [`RwLock::read`] method.
///
/// See the [module level documentation](crate::rwlock) for more.
#[must_use = "if unused the RwLock will immediately unlock"]
pub struct RwLockReadGuard<'a, T: ?Sized> {
    pub(super) lock: &'a RwLock<T>,
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



impl<'a, T: ?Sized> RwLockReadGuard<'a, T> {
    /// Makes a new [`crate::rwlock::MappedRwLockReadGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `RwLockReadGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::{RwLock, RwLockReadGuard};
    ///
    /// #[derive(Debug, Clone)]
    /// struct Foo(String);
    ///
    /// let rwlock = RwLock::new(Foo("hello".to_owned()));
    ///
    /// let guard = rwlock.read().await;
    /// let mapped_guard = RwLockReadGuard::map(guard, |f| &f.0);
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn map<U, F>(orig: Self, f: F) -> MappedRwLockReadGuard<'a, U>
    where
        F: FnOnce(&T) -> &U,
        U: ?Sized,
    {
        let d = NonNull::from(f(&*orig));
        let orig = ManuallyDrop::new(orig);
        MappedRwLockReadGuard::new(d, &orig.lock.s)
    }

    /// Attempts to make a new [`crate::rwlock::MappedRwLockReadGuard`] for a component of the locked data. The
    /// original guard is returned if the closure returns `None`.
    ///
    /// This operation cannot fail as the `RwLockReadGuard` passed in already locked the rwlock.
    ///
    /// This is an associated function that needs to be used as `RwLockReadGuard::try_map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::rwlock::{RwLock, RwLockReadGuard};
    ///
    /// #[derive(Debug, Clone)]
    /// struct Foo(String);
    ///
    /// let rwlock = RwLock::new(Foo("hello".to_owned()));
    ///
    /// let guard = rwlock.read().await;
    /// let mapped_guard = RwLockReadGuard::try_map(guard, |f| {
    ///     if f.0.len() > 3 {
    ///         Some(&f.0)
    ///     } else {
    ///         None
    ///     }
    /// }).expect("should have mapped");
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn try_map<U, F>(orig: Self, f: F) -> Result<MappedRwLockReadGuard<'a, U>, Self>
    where
        F: FnOnce(&T) -> Option<&U>,
        U: ?Sized,
    {
        match f(&*orig) {
            Some(d) => {
                let d = NonNull::from(d);
                let orig = ManuallyDrop::new(orig);
                Ok(MappedRwLockReadGuard::new(d, &orig.lock.s))
            }
            None => Err(orig),
        }
    }
}
