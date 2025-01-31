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

use crate::internal::Mutex;
use crate::internal::WaitList;
use crate::mutex;
use crate::mutex::MutexGuard;
use crate::mutex::OwnedMutexGuard;

#[cfg(test)]
mod tests;

/// A condition variable that allows tasks to wait for a notification.
///
/// See the [module level documentation](self) for more.
pub struct Condvar {
    wakers: Mutex<WaitList<WaitNode>>,
}

impl fmt::Debug for Condvar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Condvar").finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct WaitNode {
    waker: Option<Waker>,
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
            wakers: Mutex::new(WaitList::new()),
        }
    }

    /// Wakes up one blocked task on this condvar.
    pub fn notify_one(&self) {
        let mut wakers = self.wakers.lock();
        if let Some(waker) = wakers.remove_first_waiter(|_| true) {
            if let Some(waker) = waker.waker.take() {
                waker.wake();
            } else {
                unreachable!("waker was removed from the list without a waker");
            }
        }
    }

    /// Wakes up all blocked tasks on this condvar.
    pub fn notify_all(&self) {
        let mut wakers = self.wakers.lock();
        let mut waiters = vec![];
        while let Some(waker) = wakers.remove_first_waiter(|_| true) {
            if let Some(waker) = waker.waker.take() {
                waiters.push(waker);
            } else {
                unreachable!("waker was removed from the list without a waker");
            }
        }
        drop(wakers);
        for waker in waiters {
            waker.wake();
        }
    }

    /// Yields the current task until this condition variable receives a notification.
    ///
    /// Unlike the std equivalent, this does not check that a single mutex is used at runtime.
    /// However, as a best practice avoid using with multiple mutexes.
    pub async fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let mutex = mutex::guard_lock(&guard);

        let fut = AwaitNotify {
            cond: self,
            guard: Some(guard),
            key: None,
        };
        fut.await;

        mutex.lock().await
    }

    /// Yields the current task until this condition variable receives a notification.
    ///
    /// Unlike the std equivalent, this does not check that a single mutex is used at runtime.
    /// However, as a best practice avoid using with multiple mutexes.
    pub async fn wait_owned<T>(&self, guard: OwnedMutexGuard<T>) -> OwnedMutexGuard<T> {
        let mutex = mutex::owned_guard_lock(&guard);

        let fut = AwaitNotify {
            cond: self,
            guard: Some(guard),
            key: None,
        };
        fut.await;

        mutex.lock_owned().await
    }

    /// Yields the current task until this condition variable receives a notification and the
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
    /// let guard = cvar
    ///     .wait_while(lock.lock().await, |started| !*started)
    ///     .await;
    /// assert!(*guard);
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

    /// Yields the current task until this condition variable receives a notification and the
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
    /// let pair = (Arc::new(Mutex::new(false)), Arc::new(Condvar::new()));
    /// let pair_clone = pair.clone();
    ///
    /// tokio::spawn(async move {
    ///     let (lock, cvar) = pair_clone;
    ///     let mut started = lock.lock_owned().await;
    ///     *started = true;
    ///     // We notify the condvar that the value has changed.
    ///     cvar.notify_one();
    /// });
    ///
    /// // Wait for the thread to start up.
    /// let (lock, cvar) = pair;
    /// // As long as the value inside the `Mutex<bool>` is `false`, we wait.
    /// let guard = cvar
    ///     .wait_while_owned(lock.lock_owned().await, |started| !*started)
    ///     .await;
    /// assert!(*guard);
    /// # }
    /// ```
    pub async fn wait_while_owned<T, F>(
        &self,
        mut guard: OwnedMutexGuard<T>,
        mut condition: F,
    ) -> OwnedMutexGuard<T>
    where
        F: FnMut(&mut T) -> bool,
    {
        while condition(&mut *guard) {
            guard = self.wait_owned(guard).await;
        }
        guard
    }
}

/// A future that waits for another task to notify the condition variable.
struct AwaitNotify<'a, G> {
    /// The condition variable that we are waiting on.
    cond: &'a Condvar,
    /// The lock used with `cond`.
    /// This will be released the first time the future is polled,
    /// after registering the context to be notified.
    guard: Option<G>,
    /// A key into the conditions variable's [`WaitList`].
    /// This is set to the index of the `Waker` for the context each time
    /// the future is polled and not completed.
    key: Option<usize>,
}

impl<G> Future for AwaitNotify<'_, G>
where
    G: Unpin,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { cond, guard, key } = self.get_mut();

        let mut wakers = cond.wakers.lock();
        match guard.take() {
            Some(..) => {
                wakers.register_waiter(key, |node| match node {
                    None => Some(WaitNode {
                        waker: Some(cx.waker().clone()),
                    }),
                    Some(node) => unreachable!("unexpected node: {:?}", node),
                });
                // the guard is dropped when we return, which frees the lock
                Poll::Pending
            }
            None => {
                if let Some(idx) = key {
                    let mut ready = false;
                    wakers.with_mut(*idx, |node| match node.waker {
                        Some(ref mut w) => {
                            if !w.will_wake(cx.waker()) {
                                w.clone_from(cx.waker());
                            }
                            false
                        }
                        None => {
                            ready = true;
                            true
                        }
                    });

                    if ready {
                        *key = None;
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

impl<G> Drop for AwaitNotify<'_, G> {
    fn drop(&mut self) {
        let mut wakers = self.cond.wakers.lock();
        if let Some(key) = self.key {
            wakers.remove_waiter(key, |_| true);
            wakers.with_mut(key, |_| true); // drop
        }
    }
}
