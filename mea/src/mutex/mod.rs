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
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::internal;

#[cfg(test)]
mod test;

/// An async mutex for protecting shared data.
///
/// See the [module level documentation](self) for more.
pub struct Mutex<T: ?Sized> {
    s: internal::Semaphore,
    c: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

impl<T> From<T> for Mutex<T> {
    fn from(t: T) -> Self {
        Self::new(t)
    }
}

impl<T: Default> Default for Mutex<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Mutex");
        match self.try_lock() {
            Some(inner) => d.field("data", &&*inner),
            None => d.field("data", &format_args!("<locked>")),
        };
        d.finish()
    }
}

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
    ///
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
    /// let mut guard = mutex.try_lock().expect("mutex is locked");
    /// *guard += 1;
    /// assert_eq!(2, *guard);
    /// ```
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.s.try_acquire(1) {
            let guard = MutexGuard { lock: self };
            Some(guard)
        } else {
            None
        }
    }

    /// Locks this mutex, causing the current task to yield until the lock has been acquired. When
    /// the lock has been acquired, this returns an [`OwnedMutexGuard`].
    ///
    /// This method is async and will yield the current task if the mutex is currently held by
    /// another task. When the mutex becomes available, the task will be woken up and given the
    /// lock.
    ///
    /// This method is identical to [`Mutex::lock`], except that the returned guard references the
    /// `Mutex` with an [`Arc`] rather than by borrowing it. Therefore, the `Mutex` must be
    /// wrapped in an `Arc` to call this method, and the guard will live for the `'static` lifetime,
    /// as it keeps the `Mutex` alive by holding an `Arc`.
    ///
    /// # Cancel safety
    ///
    /// This method uses a queue to fairly distribute locks in the order they were requested.
    /// Cancelling a call to `lock_owned` makes you lose your place in the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use std::sync::Arc;
    ///
    /// use mea::mutex::Mutex;
    ///
    /// let mutex = Arc::new(Mutex::new(1));
    ///
    /// let mut n = mutex.clone().lock_owned().await;
    /// *n = 2;
    /// # }
    /// ```
    pub async fn lock_owned(self: Arc<Self>) -> OwnedMutexGuard<T> {
        self.s.acquire(1).await;
        OwnedMutexGuard { lock: self }
    }

    /// Attempts to acquire the lock, and returns `None` if the lock is currently held somewhere
    /// else.
    ///
    /// This method is identical to [`Mutex::try_lock`], except that the returned guard references
    /// the `Mutex` with an [`Arc`] rather than by borrowing it. Therefore, the `Mutex` must be
    /// wrapped in an `Arc` to call this method, and the guard will live for the `'static` lifetime,
    /// as it keeps the `Mutex` alive by holding an `Arc`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use mea::mutex::Mutex;
    ///
    /// let mutex = Arc::new(Mutex::new(1));
    /// let mut guard = mutex.clone().try_lock_owned().expect("mutex is locked");
    /// *guard += 1;
    /// assert_eq!(2, *guard);
    /// ```
    pub fn try_lock_owned(self: Arc<Self>) -> Option<OwnedMutexGuard<T>> {
        if self.s.try_acquire(1) {
            let guard = OwnedMutexGuard { lock: self };
            Some(guard)
        } else {
            None
        }
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `Mutex` mutably, no actual locking needs to take place: the
    /// mutable borrow statically guarantees no locks exist.
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

unsafe impl<T: ?Sized + Send + Sync> Sync for MutexGuard<'_, T> {}

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

impl<'a, T: ?Sized> MutexGuard<'a, T> {
    /// Makes a new [`MappedMutexGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `MutexGuard` passed in already locked the mutex.
    ///
    /// This is an associated function that needs to be used as `MutexGuard::map(...)`. A method
    /// would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mutex::{Mutex, MutexGuard};
    ///
    /// #[derive(Debug, Clone)]
    /// struct Foo(String);
    ///
    /// let mutex = Mutex::new(Foo("hello".to_owned()));
    ///
    /// let guard = mutex.lock().await;
    /// let mapped_guard = MutexGuard::map(guard, |f| &mut f.0);
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn map<U, F>(mut orig: Self, f: F) -> MappedMutexGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
        U: ?Sized,
    {
        let d = NonNull::from(f(&mut *orig));
        let orig = ManuallyDrop::new(orig);
        MappedMutexGuard {
            d,
            s: &orig.lock.s,
            variance: PhantomData,
        }
    }

    /// Attempts to make a new [`MappedMutexGuard`] for a component of the locked data. The
    /// original guard is returned if the closure returns `None`.
    ///
    /// This operation cannot fail as the `MutexGuard` passed in already locked the mutex.
    ///
    /// This is an associated function that needs to be used as `MutexGuard::try_map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mutex::{Mutex, MutexGuard};
    ///
    /// #[derive(Debug, Clone)]
    /// struct Foo(String);
    ///
    /// let mutex = Mutex::new(Foo("hello".to_owned()));
    ///
    /// let guard = mutex.lock().await;
    /// let mapped_guard = MutexGuard::try_map(guard, |f| {
    ///     if f.0.len() > 3 {
    ///         Some(&mut f.0)
    ///     } else {
    ///         None
    ///     }
    /// }).expect("should have mapped");
    ///
    /// assert_eq!(&*mapped_guard, "hello");
    /// # }
    /// ```
    pub fn try_map<U, F>(mut orig: Self, f: F) -> Result<MappedMutexGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
        U: ?Sized,
    {
        match f(&mut *orig) {
            Some(d) => {
                let d = NonNull::from(d);
                let orig = ManuallyDrop::new(orig);
                Ok(MappedMutexGuard {
                    d,
                    s: &orig.lock.s,
                    variance: PhantomData,
                })
            }
            None => Err(orig),
        }
    }
}

/// An owned handle to a held `Mutex`.
///
/// This guard is only available from a [`Mutex`] that is wrapped in an [`Arc`]. It is identical to
/// [`MutexGuard`], except that rather than borrowing the `Mutex`, it clones the `Arc`, incrementing
/// the reference count. This means that unlike `MutexGuard`, it will have the `'static` lifetime.
///
/// As long as you have this guard, you have exclusive access to the underlying `T`. The guard
/// internally keeps a reference-counted pointer to the original `Mutex`, so even if the lock goes
/// away, the guard remains valid.
///
/// The lock is automatically released whenever the guard is dropped, at which point `lock` will
/// succeed yet again.
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct OwnedMutexGuard<T: ?Sized> {
    lock: Arc<Mutex<T>>,
}

pub(crate) fn owned_guard_lock<T: ?Sized>(guard: &OwnedMutexGuard<T>) -> Arc<Mutex<T>> {
    guard.lock.clone()
}

unsafe impl<T: ?Sized + Send + Sync> Sync for OwnedMutexGuard<T> {}

impl<T: ?Sized> Drop for OwnedMutexGuard<T> {
    fn drop(&mut self) {
        self.lock.s.release(1);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for OwnedMutexGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for OwnedMutexGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for OwnedMutexGuard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.c.get() }
    }
}

impl<T: ?Sized> DerefMut for OwnedMutexGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.c.get() }
    }
}


/// RAII structure used to release the exclusive lock on a mutex when dropped, for a mapped component of the locked data.
///
/// This structure is created by the [`map`] and [`try_map`] methods on [`MutexGuard`]. It allows you to 
/// hold a lock on a subfield of the protected data, enabling more fine-grained access control while 
/// maintaining the same locking semantics.
///
/// As long as you have this guard, you have exclusive access to the underlying `T`. The guard 
/// internally keeps a reference to the original mutex's semaphore, so the original lock is 
/// maintained until this guard is dropped.
///
/// `MappedMutexGuard` implements [`Send`] and [`Sync`]
/// when the underlying data type supports these traits, allowing it to be used across task
/// boundaries and shared between threads safely.
///
/// [`map`]: MutexGuard::map
/// [`try_map`]: MutexGuard::try_map
///
/// # Examples
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use mea::mutex::{Mutex, MutexGuard};
///
/// #[derive(Debug)]
/// struct Foo {
///     a: u32,
///     b: String,
/// }
///
/// let mutex = Mutex::new(Foo {
///     a: 1,
///     b: "hello".to_owned(),
/// });
///
/// let guard = mutex.lock().await;
/// let mapped_guard = MutexGuard::map(guard, |foo| &mut foo.a);
/// 
/// // Now we can only access the `a` field
/// assert_eq!(*mapped_guard, 1);
/// # }
/// ```
#[must_use = "if unused the Mutex will immediately unlock"]
pub struct MappedMutexGuard<'a, T: ?Sized> {
    d: NonNull<T>,
    s: &'a internal::Semaphore,
    variance: PhantomData<&'a mut T>,
}

// SAFETY: MappedMutexGuard can be safely shared between threads (Sync) when T: Sync.
// Through &MappedMutexGuard, you can only get &T, so if T itself allows sharing references
// across threads, then sharing MappedMutexGuard references is also safe.
unsafe impl<T: ?Sized + Sync> Sync for MappedMutexGuard<'_, T> {}

// SAFETY: MappedMutexGuard can be safely sent between threads when T: Send.
// The guard holds exclusive access to the data protected by the mutex lock,
// and the NonNull<T> pointer remains valid for the guard's lifetime.
// This is essential for async tasks that may be moved between threads at .await points.
unsafe impl<T: ?Sized + Send> Send for MappedMutexGuard<'_, T> {}

impl<'a, T: ?Sized> MappedMutexGuard<'a, T> {
    /// Makes a new [`MappedMutexGuard`] for a component of the locked data.
    ///
    /// This operation cannot fail as the `MappedMutexGuard` passed in already locked the mutex.
    ///
    /// This is an associated function that needs to be used as `MappedMutexGuard::map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mutex::{Mutex, MutexGuard, MappedMutexGuard};
    ///
    /// #[derive(Debug)]
    /// struct Foo {
    ///     a: u32,
    ///     b: String,
    /// }
    ///
    /// let mutex = Mutex::new(Foo {
    ///     a: 1,
    ///     b: "hello".to_owned(),
    /// });
    ///
    /// let guard = mutex.lock().await;
    /// let mapped_guard = MutexGuard::map(guard, |foo| &mut foo.b);
    /// let nested_mapped_guard = MappedMutexGuard::map(mapped_guard, |s| s.as_mut_str());
    ///
    /// assert_eq!(&*nested_mapped_guard, "hello");
    /// # }
    /// ```
    pub fn map<U, F>(mut orig: Self, f: F) -> MappedMutexGuard<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
        U: ?Sized,
    {
        // SAFETY: orig.d is a valid NonNull<T> pointer that was created from a valid reference
        // when the original MappedMutexGuard was constructed. The guard guarantees exclusive
        // access to the data through the mutex lock, so dereferencing as mutable is safe.
        let d = NonNull::from(f(unsafe { orig.d.as_mut() }));
        let orig = ManuallyDrop::new(orig);
        MappedMutexGuard {
            d,
            s: orig.s,
            variance: PhantomData,
        }
    }

    /// Attempts to make a new [`MappedMutexGuard`] for a component of the locked data. The
    /// original guard is returned if the closure returns `None`.
    ///
    /// This operation cannot fail as the `MappedMutexGuard` passed in already locked the mutex.
    ///
    /// This is an associated function that needs to be used as `MappedMutexGuard::try_map(...)`. A
    /// method would interfere with methods of the same name on the contents of the locked data.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use mea::mutex::{Mutex, MutexGuard, MappedMutexGuard};
    ///
    /// #[derive(Debug)]
    /// struct Foo {
    ///     a: u32,
    ///     b: String,
    /// }
    ///
    /// let mutex = Mutex::new(Foo {
    ///     a: 1,
    ///     b: "hello".to_owned(),
    /// });
    ///
    /// let guard = mutex.lock().await;
    /// let mapped_guard = MutexGuard::map(guard, |foo| &mut foo.b);
    /// let nested_mapped_guard = MappedMutexGuard::try_map(mapped_guard, |s| {
    ///     if s.len() > 3 {
    ///         Some(s.as_mut_str())
    ///     } else {
    ///         None
    ///     }
    /// }).expect("should have mapped");
    ///
    /// assert_eq!(&*nested_mapped_guard, "hello");
    /// # }
    /// ```
    pub fn try_map<U, F>(mut orig: Self, f: F) -> Result<MappedMutexGuard<'a, U>, Self>
    where
        F: FnOnce(&mut T) -> Option<&mut U>,
        U: ?Sized,
    {
        // SAFETY: orig.d is a valid NonNull<T> pointer that was created from a valid reference
        // when the original MappedMutexGuard was constructed. The guard guarantees exclusive
        // access to the data through the mutex lock, so dereferencing as mutable is safe.
        match f(unsafe { orig.d.as_mut() }) {
            Some(d) => {
                let d = NonNull::from(d);
                let orig = ManuallyDrop::new(orig);
                Ok(MappedMutexGuard {
                    d,
                    s: orig.s,
                    variance: PhantomData,
                })
            }
            None => Err(orig),
        }
    }
}

impl<T: ?Sized> Drop for MappedMutexGuard<'_, T> {
    fn drop(&mut self) {
        self.s.release(1);
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for MappedMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Display> fmt::Display for MappedMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized> Deref for MappedMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // SAFETY: we hold the lock and the NonNull pointer is valid for the guard's lifetime
        unsafe { self.d.as_ref() }
    }
}

impl<T: ?Sized> DerefMut for MappedMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: we hold the lock and the NonNull pointer is valid for the guard's lifetime
        unsafe { self.d.as_mut() }
    }
}
