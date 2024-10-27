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

use crate::internal::WaitQueueSync;
use alloc::sync::Arc;
use core::fmt;
use core::future::Future;
use core::future::IntoFuture;
use core::pin::Pin;
use core::task::Context;
use core::task::Poll;

#[cfg(test)]
mod tests;

pub struct WaitGroup {
    sync: Arc<WaitQueueSync>,
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitGroup {
    pub fn new() -> Self {
        Self {
            sync: Arc::new(WaitQueueSync::new(1)),
        }
    }
}

impl Clone for WaitGroup {
    fn clone(&self) -> Self {
        let sync = self.sync.clone();
        let mut cnt = sync.state();
        loop {
            let new_cnt = cnt.saturating_add(1);
            match sync.cas_state(cnt, new_cnt) {
                Ok(_) => return Self { sync },
                Err(x) => cnt = x,
            }
        }
    }
}

impl Drop for WaitGroup {
    fn drop(&mut self) {
        self.sync.release_shared(1, |sync, _| {
            let mut cnt = sync.state();
            loop {
                if cnt == 0 {
                    return false;
                }
                let new_cnt = cnt.saturating_sub(1);
                match sync.cas_state(cnt, new_cnt) {
                    Ok(_) => return new_cnt == 0,
                    Err(x) => cnt = x,
                }
            }
        });
    }
}

impl IntoFuture for WaitGroup {
    type Output = ();

    type IntoFuture = WaitGroupFuture;

    fn into_future(self) -> Self::IntoFuture {
        let sync = self.sync.clone();
        drop(self);
        WaitGroupFuture { sync }
    }
}

impl fmt::Debug for WaitGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitGroup").finish_non_exhaustive()
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WaitGroupFuture {
    sync: Arc<WaitQueueSync>,
}

impl Future for WaitGroupFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        fn try_acquire_shared(sync: &WaitQueueSync, _: u32) -> bool {
            sync.state() == 0
        }

        let Self { sync } = self.get_mut();
        if sync.acquire_shared(cx, 1, try_acquire_shared) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
