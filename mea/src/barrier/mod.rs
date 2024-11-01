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

use crate::internal::Mutex;
use crate::internal::WaitSet;

#[cfg(test)]
mod tests;

pub struct Barrier {
    n: u32,
    state: Mutex<BarrierState>,
}

struct BarrierState {
    arrived: u32,
    generation: usize,
    waiters: WaitSet,
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
            state: Mutex::new(BarrierState {
                arrived: 0,
                generation: 0,
                waiters: WaitSet::with_capacity(n as usize),
            }),
        }
    }

    /// Returns a future that suspends the current task to wait until the counter reaches zero.
    ///
    /// The output of the future is a bool indicates whether this waiter is the leader (last one).
    pub async fn wait(&self) -> bool {
        let generation = {
            let mut state = self.state.lock();
            let generation = state.generation;
            state.arrived += 1;

            // the last arriver is the leader;
            // wake up other waiters, increment the generation, and return
            if state.arrived == self.n {
                state.arrived = 0;
                state.generation += 1;
                state.waiters.wake_all();
                return true;
            }

            generation
        };

        let fut = BarrierWait {
            idx: None,
            generation,
            barrier: self,
        };
        fut.await;
        false
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
struct BarrierWait<'a> {
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

        let mut state = barrier.state.lock();
        if *generation < state.generation {
            Poll::Ready(())
        } else {
            state.waiters.register_waker(idx, cx);
            Poll::Pending
        }
    }
}
