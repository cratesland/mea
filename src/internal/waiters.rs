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

use crate::internal::Mutex;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::take;
use core::task::Waker;

// Wake-on-Drop waiters
pub(crate) struct WoDWaiters {
    data: Vec<Waker>,
}

impl WoDWaiters {
    pub(crate) const fn new() -> Self {
        Self { data: vec![] }
    }

    pub(crate) fn upsert(&mut self, id: &mut Option<usize>, waker: &Waker) {
        match *id {
            Some(key) => match self.data.get_mut(key) {
                Some(w) => {
                    if !w.will_wake(waker) {
                        *w = waker.clone()
                    }
                }
                // SAFETY: The outer container should guarantee
                //   1. The value of "id" is one returned by previous "upsert" call.
                //   2. `wake_all` is called on drop, and no more `upsert` will be called
                //      once the outer container is being dropped.
                _ => unreachable!("[BUG] update non-existent waker"),
            },
            None => {
                self.data.push(waker.clone());
                *id = Some(self.data.len());
            }
        }
    }

    // SAFETY: Always call this method when the outer container is being dropped.
    pub(crate) fn wake_all(mutex: &Mutex<Self>) {
        let waker_list = mutex.with(|lock| take(&mut lock.data));
        for w in waker_list {
            w.wake();
        }
    }
}
