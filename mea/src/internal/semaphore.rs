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

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::sync::MutexGuard;
use std::task::Context;
use std::task::Poll;
use std::task::Waker;

use crate::internal::Mutex;
use crate::internal::WaitList;

/// The internal semaphore that provides low-level async primitives.
#[derive(Debug)]
pub(crate) struct Semaphore {
    /// The current number of available permits in the semaphore.
    permits: AtomicU32,
    waiters: Mutex<WaitList<WaitNode>>,
}

#[derive(Debug)]
struct WaitNode {
    permits: u32,
    waker: Option<Waker>,
}

impl Semaphore {
    pub(crate) fn new(permits: u32) -> Self {
        Self {
            permits: AtomicU32::new(permits),
            waiters: Mutex::new(WaitList::new()),
        }
    }

    /// Returns the current number of available permits.
    pub(crate) fn available_permits(&self) -> u32 {
        self.permits.load(Ordering::Acquire)
    }

    /// Tries to acquire `n` permits from the semaphore.
    ///
    /// Returns `true` if the permits were acquired, `false` otherwise.
    pub(crate) fn try_acquire(&self, n: u32) -> bool {
        let mut current = self.permits.load(Ordering::Acquire);
        loop {
            if current < n {
                return false;
            }

            let next = current - n;
            match self
                .permits
                .compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Decrease a semaphore's permits by a maximum of `n`.
    ///
    /// Return the number of permits that were actually reduced.
    pub(crate) fn forget(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }

        let mut current = self.permits.load(Ordering::Acquire);
        loop {
            let new = current.saturating_sub(n);
            match self.permits.compare_exchange_weak(
                current,
                new,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return n.min(current),
                Err(actual) => current = actual,
            }
        }
    }

    /// Acquires `n` permits from the semaphore.
    pub(crate) async fn acquire(&self, n: u32) {
        let fut = Acquire {
            permits: n,
            index: None,
            semaphore: self,
            done: false,
        };
        fut.await
    }

    /// Adds `n` new permits to the semaphore.
    pub(crate) fn release(&self, n: u32) {
        if n != 0 {
            self.insert_permits_with_lock(n, self.waiters.lock());
        }
    }

    fn insert_permits_with_lock(&self, mut rem: u32, waiters: MutexGuard<'_, WaitList<WaitNode>>) {
        const NUM_WAKER: usize = 32;
        let mut wakers = Vec::with_capacity(NUM_WAKER);

        let mut lock = Some(waiters);
        while rem > 0 {
            let mut waiters = lock.take().unwrap_or_else(|| self.waiters.lock());
            while wakers.len() < NUM_WAKER {
                match waiters.remove_first_waiter(|node| {
                    if node.permits <= rem {
                        rem -= node.permits;
                        node.permits = 0;
                        true
                    } else {
                        node.permits -= rem;
                        rem = 0;
                        false
                    }
                }) {
                    None => break,
                    Some(waiter) => {
                        if let Some(waker) = waiter.waker.take() {
                            wakers.push(waker);
                        } else {
                            unreachable!("waker was removed from the list without a waker");
                        }
                    }
                }
            }

            if rem > 0 && waiters.is_empty() {
                let permits = rem;
                let prev = self.permits.fetch_add(permits, Ordering::Release);
                assert!(
                    prev.checked_add(permits).is_some(),
                    "number of added permits ({permits}) would overflow u32::MAX (prev: {prev})"
                );
                rem = 0;
            }

            drop(waiters);
            for w in wakers {
                w.wake();
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct Acquire<'a> {
    permits: u32,
    index: Option<usize>,
    semaphore: &'a Semaphore,
    done: bool,
}

impl Drop for Acquire<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index {
            let mut waiters = self.semaphore.waiters.lock();
            let mut acquired = 0;
            waiters.remove_waiter(index, |node| {
                acquired = node.permits;
                node.permits = 0;
                true
            });
            waiters.with_mut(index, |_| true); // drop
            if acquired > 0 {
                self.semaphore.insert_permits_with_lock(acquired, waiters);
            }
        }
    }
}

impl Future for Acquire<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            permits,
            index,
            semaphore,
            done,
        } = self.get_mut();

        if *done {
            return Poll::Ready(());
        }

        match index {
            Some(idx) => {
                let mut waiters = semaphore.waiters.lock();
                let mut ready = false;
                waiters.with_mut(*idx, |node| {
                    if node.permits > 0 {
                        let update_waker = node
                            .waker
                            .as_ref()
                            .map_or(true, |w| !w.will_wake(cx.waker()));
                        if update_waker {
                            node.waker = Some(cx.waker().clone());
                        }
                        false
                    } else {
                        ready = true;
                        true
                    }
                });

                if ready {
                    *index = None;
                    *done = true;
                    return Poll::Ready(());
                }
            }
            None => {
                // not yet enqueued
                let needed = *permits;

                let mut acquired = 0;
                let mut current = semaphore.permits.load(Ordering::Acquire);
                let mut lock = None;

                let mut waiters = loop {
                    let mut remaining = 0;
                    let total = current.checked_add(acquired).expect("permits overflow");
                    let (next, acq) = if total >= needed {
                        let next = current - (needed - acquired);
                        (next, needed - acquired)
                    } else {
                        remaining = (needed - acquired) - current;
                        (0, current)
                    };

                    if remaining > 0 && lock.is_none() {
                        // No permits were immediately available, so this permit will
                        // (probably) need to wait. We'll need to acquire a lock on the
                        // wait queue before continuing. We need to do this _before_ the
                        // CAS that sets the new value of the semaphore's `permits`
                        // counter. Otherwise, if we subtract the permits and then
                        // acquire the lock, we might miss additional permits being
                        // added while waiting for the lock.
                        lock = Some(semaphore.waiters.lock());
                    }

                    match semaphore.permits.compare_exchange(
                        current,
                        next,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => {
                            acquired += acq;
                            if remaining == 0 {
                                *done = true;
                                return Poll::Ready(());
                            }
                            break lock.expect("lock not acquired");
                        }
                        Err(actual) => current = actual,
                    }
                };

                waiters.register_waiter(index, |node| match node {
                    None => Some(WaitNode {
                        permits: needed - acquired,
                        waker: Some(cx.waker().clone()),
                    }),
                    Some(node) => unreachable!("unexpected node: {:?}", node),
                });
            }
        };

        Poll::Pending
    }
}
