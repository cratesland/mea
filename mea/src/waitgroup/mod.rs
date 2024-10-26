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

use alloc::sync::Arc;
use alloc::sync::Weak;
use core::future::Future;
use core::future::IntoFuture;
use core::pin::Pin;
use core::task::Context;
use core::task::Poll;

use crate::internal::Mutex;
use crate::internal::WakeOnDropWaitSet;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct WaitGroup {
    inner: Arc<Inner>,
}

impl WaitGroup {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                waiters: Mutex::new(WakeOnDropWaitSet::new()),
            }),
        }
    }
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoFuture for WaitGroup {
    type Output = ();

    type IntoFuture = WaitGroupFuture;

    fn into_future(self) -> Self::IntoFuture {
        WaitGroupFuture {
            id: None,
            inner: Arc::downgrade(&self.inner),
        }
    }
}

struct Inner {
    waiters: Mutex<WakeOnDropWaitSet>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        WakeOnDropWaitSet::wake_all(&self.waiters);
    }
}

/// Future returned by [`WaitGroup::into_future`].
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct WaitGroupFuture {
    id: Option<usize>,
    inner: Weak<Inner>,
}

impl Future for WaitGroupFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { id, inner } = self.get_mut();
        match inner.upgrade() {
            Some(inner) => {
                inner.waiters.with(|waiters| {
                    waiters.upsert(id, cx.waker());
                });
                Poll::Pending
            }
            None => Poll::Ready(()),
        }
    }
}
