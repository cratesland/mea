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
use crate::internal::Waiters;

#[derive(Clone)]
pub struct WaitGroup {
    inner: Arc<Inner>,
}

impl WaitGroup {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                waiters: Mutex::new(Waiters::new()),
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
    waiters: Mutex<Waiters>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        Waiters::wake_all(&self.waiters);
    }
}

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
                let mut lock = inner.waiters.lock();
                lock.upsert(id, cx.waker());
                Poll::Pending
            }
            None => Poll::Ready(()),
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn test_wait_group_drop() {
        let test_runtime = test_runtime();

        let wg = WaitGroup::new();
        for _i in 0..100 {
            let w = wg.clone();
            test_runtime.spawn(async move {
                drop(w);
            });
        }
        pollster::block_on(wg.into_future());
    }

    #[test]
    fn test_wait_group_await() {
        let test_runtime = test_runtime();

        let wg = WaitGroup::new();
        for _i in 0..100 {
            let w = wg.clone();
            test_runtime.spawn(async move {
                w.await;
            });
        }
        pollster::block_on(wg.into_future());
    }

    #[test]
    fn test_wait_group_timeout() {
        let test_runtime = test_runtime();

        let wg = WaitGroup::new();
        let _wg_clone = wg.clone();
        let timeout = test_runtime.block_on(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(50)) => true ,
                _ = wg => false,
            }
        });
        assert!(timeout);
    }
}
