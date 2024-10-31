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
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use crate::internal::CountdownState;

#[cfg(test)]
mod tests;

pub struct Latch {
    state: CountdownState,
}

impl fmt::Debug for Latch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Latch")
            .field("count", &self.count())
            .finish_non_exhaustive()
    }
}

impl Latch {
    /// Constructs a `Latch` initialized with the given count.
    pub fn new(count: u32) -> Self {
        Self {
            state: CountdownState::new(count),
        }
    }

    /// Returns the current count.
    ///
    /// This method is typically used for debugging and testing purposes.
    pub fn count(&self) -> u32 {
        self.state.state()
    }

    /// Decrements the latch count, wake up all pending tasks if the counter reaches zero.
    ///
    /// If the current count is greater than zero then it is decremented. If the new count is zero
    /// then all pending tasks are waken up.
    ///
    /// If the current count equals zero then nothing happens.
    pub fn count_down(&self) {
        if self.state.decrement(1) {
            self.state.wake_all();
        }
    }

    /// Decrements the latch count by `n`, re-enable all waiting threads if the
    /// counter reaches zero after decrement.
    ///
    /// It will not cause an overflow by decrement the counter.
    ///
    /// * If the `n` is zero or the counter has reached zero then do nothing.
    /// * If the current count is greater than `n` then decremented by `n`.
    /// * If the current count is greater than 0 and less than or equal to `n`, then the new count
    ///   will be zero, and all waiting threads are re-enabled.
    pub fn arrive(&self, n: u32) {
        if n != 0 && self.state.decrement(n) {
            self.state.wake_all();
        }
    }

    /// Checks that the current counter has reached zero.
    ///
    /// # Errors
    ///
    /// This function will return an error with the current count if the
    /// counter has not reached zero.
    pub fn try_wait(&self) -> Result<(), u32> {
        self.state.spin_wait(0)
    }

    /// Returns a future that suspends the current task to wait until the counter reaches zero.
    pub fn wait(&self) -> LatchWait<'_> {
        LatchWait {
            idx: None,
            latch: self,
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LatchWait<'a> {
    idx: Option<usize>,
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
        let Self { idx, latch } = self.get_mut();

        // register waker if the counter is not zero
        if latch.state.spin_wait(16).is_err() {
            latch.state.register_waker(idx, cx);
            // double check after register waker, to catch the update between two steps
            if latch.state.spin_wait(0).is_err() {
                return Poll::Pending;
            }
        }

        Poll::Ready(())
    }
}
