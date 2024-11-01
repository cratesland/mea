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

use crate::internal::{CountdownState, Mutex};

#[cfg(test)]
mod tests;

pub struct Barrier {
    n: u32,
    g: Mutex<usize>, // generation
    state: CountdownState,
}

impl fmt::Debug for Barrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Barrier").finish_non_exhaustive()
    }
}

impl Barrier {
    /// Constructs a `Barrier` that can block a given number of tasks.
    pub fn new(n: u32) -> Self {
        // If n is 0, it's not clear what behavior the user wants.
        // std::sync::Barrier works with n = 0 the same as n = 1,
        // where every .wait() immediately unblocks, so we adopt that here as well.
        let n = n.max(1);
        Self {
            n,
            g: Mutex::new(0),
            state: CountdownState::new(n),
        }
    }

    /// Returns a future that suspends the current task to wait until the counter reaches zero.
    ///
    /// The output of the future is a bool indicates whether this waiter is the leader (last one).
    pub async fn wait(&self) -> bool {
        if self.state.decrement(1) {
            let mut g = self.g.lock();
            *g += 1;
            self.state.reset(self.n);
            return true;
        }

        let fut = {
            let g = self.g.lock();
            BarrierWait {
                idx: None,
                generation: *g,
                barrier: self,
            }
            // drop(g);
        };
        fut.await;
        false
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct BarrierWait<'a> {
    idx: Option<usize>,
    generation: usize,
    barrier: &'a Barrier,
}

impl fmt::Debug for BarrierWait<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BarrierWait")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Future for BarrierWait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            idx,
            generation,
            barrier,
        } = self.get_mut();

        let g = barrier.g.lock();
        if *generation < *g {
            Poll::Ready(())
        } else {
            barrier.state.register_waker(idx, cx);
            Poll::Pending
        }
    }
}
