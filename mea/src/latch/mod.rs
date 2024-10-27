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
use core::task::Context;
use core::task::Poll;

use crate::internal::WaitQueueSync;

#[cfg(test)]
mod tests;

pub struct Latch {
    sync: WaitQueueSync,
}

impl Latch {
    /// Constructs a `Latch` initialized with the given count.
    pub const fn new(count: u32) -> Self {
        Self {
            sync: WaitQueueSync::new(count),
        }
    }

    /// Returns the current count.
    ///
    /// This method is typically used for debugging and testing purposes.
    pub fn count(&self) -> u32 {
        self.sync.state()
    }

    /// Decrements the latch count, wake up all pending tasks if the counter reaches zero.
    ///
    /// If the current count is greater than zero then it is decremented. If the new count is zero
    /// then all pending tasks are waken up.
    ///
    /// If the current count equals zero then nothing happens.
    pub fn count_down(&self) {
        self.sync.release_shared_by_one();
    }

    /// Returns a future that suspends the current task to wait until the counter reaches zero.
    pub const fn wait(&self) -> LatchWait<'_> {
        LatchWait { latch: self }
    }
}

impl fmt::Debug for Latch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Latch")
            .field("count", &self.count())
            .finish_non_exhaustive()
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LatchWait<'a> {
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
        let Self { latch } = self.get_mut();
        if latch.sync.acquire_shared_on_state_is_zero(cx) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
