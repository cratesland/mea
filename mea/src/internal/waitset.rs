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

use slab::Slab;
use std::task::{Context, Waker};

#[derive(Debug)]
pub(crate) struct WaitSet {
    waiters: Slab<Waker>,
}

impl WaitSet {
    /// Construct a new, empty wait set.
    pub const fn new() -> Self {
        Self {
            waiters: Slab::new(),
        }
    }

    /// Construct a new, empty wait set with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            waiters: Slab::with_capacity(capacity),
        }
    }

    /// Drain and wake up all waiters.
    pub(crate) fn wake_all(&mut self) {
        let waiters = std::mem::take(&mut self.waiters);
        for (_, waker) in waiters.into_iter() {
            waker.wake();
        }
    }

    /// Registers a waker to the wait set.
    ///
    /// `idx` must be `None` when the waker is not registered, or `Some(key)` where `key` is
    /// a value previously returned by this method.
    pub(crate) fn register_waker(&mut self, idx: &mut Option<usize>, cx: &mut Context<'_>) {
        match *idx {
            None => {
                let key = self.waiters.insert(cx.waker().clone());
                *idx = Some(key);
            }
            Some(key) => {
                if !self.waiters[key].will_wake(cx.waker()) {
                    self.waiters[key] = cx.waker().clone();
                }
            }
        }
    }
}
