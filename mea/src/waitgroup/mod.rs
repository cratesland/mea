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
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use crate::internal::CountdownState;

#[cfg(test)]
mod tests;

pub struct WaitGroup {
    state: Arc<CountdownState>,
}

impl fmt::Debug for WaitGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitGroup").finish_non_exhaustive()
    }
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitGroup {
    pub fn new() -> Self {
        Self {
            state: Arc::new(CountdownState::new(1)),
        }
    }
}

impl Clone for WaitGroup {
    fn clone(&self) -> Self {
        let sync = self.state.clone();
        let mut cnt = sync.state();
        loop {
            let new_cnt = cnt.saturating_add(1);
            match sync.cas_state(cnt, new_cnt) {
                Ok(_) => return Self { state: sync },
                Err(x) => cnt = x,
            }
        }
    }
}

impl Drop for WaitGroup {
    fn drop(&mut self) {
        if self.state.decrement(1) {
            self.state.wake_all();
        }
    }
}

impl IntoFuture for WaitGroup {
    type Output = ();

    type IntoFuture = WaitGroupFuture;

    fn into_future(self) -> Self::IntoFuture {
        let state = self.state.clone();
        drop(self);
        WaitGroupFuture { idx: None, state }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WaitGroupFuture {
    idx: Option<usize>,
    state: Arc<CountdownState>,
}

impl fmt::Debug for WaitGroupFuture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitGroupFuture").finish_non_exhaustive()
    }
}

impl Future for WaitGroupFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { idx, state } = self.get_mut();

        // register waker if the counter is not zero
        if state.spin_wait(16).is_err() {
            state.register_waker(idx, cx);
            // double check after register waker, to catch the update between two steps
            if state.spin_wait(0).is_err() {
                return Poll::Pending;
            }
        }

        Poll::Ready(())
    }
}
