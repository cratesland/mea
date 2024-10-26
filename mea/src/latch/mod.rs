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

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::task::Context;
use core::task::Poll;

use crate::internal::Mutex;
use crate::internal::WakeOnDropWaitSet;

#[cfg(test)]
mod tests;

pub struct Latch {
    stat: AtomicUsize,
    waiters: Mutex<WakeOnDropWaitSet>,
}

impl fmt::Debug for Latch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Latch")
            .field("count", &self.stat)
            .finish_non_exhaustive()
    }
}

impl Latch {
    /// Creates a new latch initialized with the given count.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(10);
    /// # drop(latch);
    /// ```
    pub const fn new(count: usize) -> Self {
        Self {
            stat: AtomicUsize::new(count),
            waiters: Mutex::new(WakeOnDropWaitSet::new()),
        }
    }

    /// Decrements the latch count, wake up all pending tasks if the counter
    /// reaches 0 after decrement.
    ///
    /// - If the counter has reached 0 then do nothing.
    /// - If the current count is greater than 0 then it is decremented.
    /// - If the new count is 0 then all pending tasks are waked up.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(1);
    /// latch.count_down();
    /// ```
    pub fn count_down(&self) {
        self.decrement(1);
    }

    /// Decrements the latch count by `n`, wake up all pending tasks if the
    /// counter reaches 0 after decrement.
    ///
    /// It won't cause any overflow by decrement the counter:
    ///
    /// * If the `n` is 0 or the counter has reached 0 then do nothing.
    /// * If the current count is greater than `n` then decremented by `n`.
    /// * If the current count is greater than 0 and less than or equal to `n`, then the new count
    ///   will be 0, and all pending tasks are waked up.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(10);
    ///
    /// // Execute a batch upsert SQL and return `updated_rows` = 10 in runtime.
    /// # let updated_rows = 10;
    /// latch.arrive(updated_rows);
    /// assert_eq!(latch.count(), 0);
    /// ```
    pub fn arrive(&self, n: usize) {
        if n > 0 {
            self.decrement(n);
        }
    }

    /// Acquires the current count.
    ///
    /// It is typically used for debugging and testing.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(3);
    /// assert_eq!(latch.count(), 3);
    /// ```
    #[must_use]
    #[inline]
    pub fn count(&self) -> usize {
        self.stat.load(Ordering::Acquire)
    }

    /// Returns a future that suspends the current task to wait until the
    /// counter reaches 0.
    ///
    /// When the future is polled:
    ///
    /// * If the current count is 0 then ready immediately.
    /// * If the current count is greater than 0 then pending with a waker that will be awakened by
    ///   a [`count_down()`]/[`arrive()`] invocation which causes the counter reaches 0.
    ///
    /// [`count_down()`]: Latch::count_down
    /// [`arrive()`]: Latch::arrive
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::runtime::Builder;
    /// use std::sync::Arc;
    /// use std::thread;
    ///
    /// use mea::latch::Latch;
    ///
    /// # Builder::new_multi_thread().build().unwrap().block_on(async move {
    /// let latch = Arc::new(Latch::new(1));
    /// let latch_clone = latch.clone();
    ///
    /// thread::spawn(move || latch_clone.count_down());
    /// latch.wait().await;
    /// # });
    /// ```
    pub const fn wait(&self) -> LatchWait<'_> {
        LatchWait {
            id: None,
            latch: self,
        }
    }

    fn decrement(&self, n: usize) {
        let mut cnt = self.stat.load(Ordering::Relaxed);
        while cnt != 0 {
            // avoid undefined behavior due to overflow
            let new_cnt = if cnt < n { 0 } else { cnt - n };
            match self.stat.compare_exchange_weak(
                cnt,
                new_cnt,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    if new_cnt == 0 {
                        self.done();
                    }
                    break;
                }
                Err(x) => cnt = x,
            }
        }
    }

    fn spin(&self) -> bool {
        let mut stat = self.stat.load(Ordering::Acquire);
        for _ in 0..16 {
            if stat == 0 {
                return true;
            }
            core::hint::spin_loop();
            stat = self.stat.load(Ordering::Acquire);
        }
        false
    }

    fn done(&self) {
        WakeOnDropWaitSet::wake_all(&self.waiters);
    }
}

/// Future returned by [`Latch::wait`].
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LatchWait<'a> {
    id: Option<usize>,
    latch: &'a Latch,
}

impl fmt::Debug for LatchWait<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LatchWait").finish_non_exhaustive()
    }
}

impl Future for LatchWait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { latch, id } = self.get_mut();

        // Spin first to speed things up if the lock is released quickly.
        if latch.spin() {
            Poll::Ready(())
        } else {
            latch.waiters.with(|waiters| {
                if latch.stat.load(Ordering::Acquire) == 0 {
                    Poll::Ready(())
                } else {
                    waiters.upsert(id, cx.waker());
                    Poll::Pending
                }
            })
        }
    }
}
