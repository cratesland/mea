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

//! A condition variable that allows tasks to wait for a notification.
//!
//! # Examples
//!
//! ```
//! # #[tokio::main]
//! # async fn main() {
//! use std::sync::Arc;
//!
//! use mea::condvar::Condvar;
//! use mea::mutex::Mutex;
//!
//! let pair = Arc::new((Mutex::new(false), Condvar::new()));
//! let pair_clone = pair.clone();
//!
//! // Inside our lock, spawn a new thread, and then wait for it to start.
//! tokio::spawn(async move {
//!     let (lock, cvar) = &*pair_clone;
//!     let mut started = lock.lock().await;
//!     *started = true;
//!     // We notify the condvar that the value has changed.
//!     cvar.notify_one();
//! });
//!
//! // Wait for the thread to start up.
//! let (lock, cvar) = &*pair;
//! let mut started = lock.lock().await;
//! while !*started {
//!     started = cvar.wait(started).await;
//! }
//! # }
//! ```

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use slab::Slab;

use crate::internal::Mutex;
use crate::mutex::MutexGuard;

#[cfg(test)]
mod tests;

/// A condition variable that allows tasks to wait for a notification.
///
/// See the [module level documentation](self) for more.
pub struct Condvar {
    wakers: Mutex<WakerSet>,
}

impl fmt::Debug for Condvar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Condvar").finish_non_exhaustive()
    }
}

impl Default for Condvar {
    fn default() -> Self {
        Self::new()
    }
}

impl Condvar {
    /// Creates a new condition variable
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::condvar::Condvar;
    ///
    /// let cvar = Condvar::new();
    /// ```
    pub fn new() -> Condvar {
        Condvar {
            wakers: Mutex::new(WakerSet {
                entries: Slab::new(),
                notifiable: 0,
            }),
        }
    }

    /// Wakes up one blocked task on this condvar.
    pub fn notify_one(&self) {
        let mut wakers = self.wakers.lock();
        wakers.notify_one();
    }

    /// Wakes up all blocked tasks on this condvar.
    pub fn notify_all(&self) {
        let mut wakers = self.wakers.lock();
        wakers.notify_all();
    }

    /// Blocks the current task until this condition variable receives a notification.
    ///
    /// Unlike the std equivalent, this does not check that a single mutex is used at runtime.
    /// However, as a best practice avoid using with multiple mutexes.
    pub async fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let mutex = MutexGuard::source(&guard);

        let fut = AwaitNotify {
            cond: self,
            guard: Some(guard),
            key: None,
        };
        fut.await;

        mutex.lock().await
    }

    /// Blocks the current task until this condition variable receives a notification and the
    /// provided condition becomes false. Spurious wake-ups are ignored and this function will only
    /// return once the condition has been met.
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use std::sync::Arc;
    ///
    /// use mea::condvar::Condvar;
    /// use mea::mutex::Mutex;
    ///
    /// let pair = Arc::new((Mutex::new(false), Condvar::new()));
    /// let pair_clone = pair.clone();
    ///
    /// tokio::spawn(async move {
    ///     let (lock, cvar) = &*pair_clone;
    ///     let mut started = lock.lock().await;
    ///     *started = true;
    ///     // We notify the condvar that the value has changed.
    ///     cvar.notify_one();
    /// });
    ///
    /// // Wait for the thread to start up.
    /// let (lock, cvar) = &*pair;
    /// // As long as the value inside the `Mutex<bool>` is `false`, we wait.
    /// let _guard = cvar
    ///     .wait_while(lock.lock().await, |started| !*started)
    ///     .await;
    /// # }
    /// ```
    pub async fn wait_while<'a, T, F>(
        &self,
        mut guard: MutexGuard<'a, T>,
        mut condition: F,
    ) -> MutexGuard<'a, T>
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut *guard) {
            guard = self.wait(guard).await;
        }
        guard
    }
}

/// A future that waits for another task to notify the condition variable.
struct AwaitNotify<'a, 'b, T> {
    /// The condition variable that we are waiting on.
    cond: &'a Condvar,
    /// The lock used with `cond`.
    /// This will be released the first time the future is polled,
    /// after registering the context to be notified.
    guard: Option<MutexGuard<'b, T>>,
    /// A key into the conditions variable's [`WakerSet`].
    /// This is set to the index of the `Waker` for the context each time
    /// the future is polled and not completed.
    key: Option<usize>,
}

impl<T> Future for AwaitNotify<'_, '_, T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut wakers = self.cond.wakers.lock();
        match self.guard.take() {
            Some(_) => {
                self.key = Some(wakers.insert(cx));
                // the guard is dropped when we return, which frees the lock
                Poll::Pending
            }
            None => {
                if let Some(key) = self.key {
                    if wakers.remove_if_notified(key, cx) {
                        self.key = None;
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                } else {
                    // This should only happen if it is polled twice after receiving a notification
                    Poll::Ready(())
                }
            }
        }
    }
}

impl<T> Drop for AwaitNotify<'_, '_, T> {
    fn drop(&mut self) {
        let mut wakers = self.cond.wakers.lock();
        if let Some(key) = self.key {
            wakers.cancel(key);
        }
    }
}

struct WakerSet {
    entries: Slab<Option<Waker>>,
    notifiable: usize,
}

impl WakerSet {
    /// Inserts a waker for a blocked operation and returns a key associated with it.
    fn insert(&mut self, cx: &Context<'_>) -> usize {
        let key = self.entries.insert(Some(cx.waker().clone()));
        self.notifiable += 1;
        key
    }

    /// If the waker for this key is still waiting for a notification, then update
    /// the waker for the entry, and return false. If the waker has been notified,
    /// treat the entry as completed and return true.
    fn remove_if_notified(&mut self, key: usize, cx: &Context<'_>) -> bool {
        match &mut self.entries[key] {
            None => {
                self.entries.remove(key);
                true
            }
            Some(w) => {
                // We were never woken, so update instead
                if !w.will_wake(cx.waker()) {
                    *w = cx.waker().clone();
                }
                false
            }
        }
    }

    /// Notifies all blocked operations.
    ///
    /// Returns `true` if at least one operation was notified.
    fn notify_all(&mut self) -> bool {
        if self.notifiable > 0 {
            let mut notified = false;
            for (_, opt_waker) in self.entries.iter_mut() {
                if let Some(w) = opt_waker.take() {
                    w.wake();
                    self.notifiable -= 1;
                    notified = true;
                }
            }
            assert_eq!(self.notifiable, 0);
            notified
        } else {
            false
        }
    }

    /// Notifies one additional blocked operation.
    ///
    /// Returns `true` if an operation was notified.
    fn notify_one(&mut self) -> bool {
        if self.notifiable > 0 {
            for (_, opt_waker) in self.entries.iter_mut() {
                if let Some(w) = opt_waker.take() {
                    w.wake();
                    self.notifiable -= 1;
                    return true;
                }
            }
        }
        false
    }

    /// Removes the waker of a cancelled operation.
    ///
    /// Returns `true` if another blocked operation from the set was notified.
    fn cancel(&mut self, key: usize) -> bool {
        match self.entries.remove(key) {
            Some(_) => {
                self.notifiable -= 1;
                false
            }
            // The operation was cancelled and notified so notify another operation instead.
            // If there is no waker in this entry, that means it was already woken.
            None => self.notify_one(),
        }
    }
}
