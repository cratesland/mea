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
use alloc::vec::Vec;
use core::mem::take;
use core::task::Waker;

/// A lightweight slab implementation.
pub(crate) struct Waiters {
    data: Vec<Waiter>,
    next: usize,
}

enum Waiter {
    Occupied(Waker),
    Vacant(usize),
}

impl Waiters {
    pub(crate) const fn new() -> Self {
        Self {
            data: Vec::new(),
            next: 0,
        }
    }

    pub(crate) fn upsert(&mut self, id: &mut Option<usize>, waker: &Waker) {
        match *id {
            Some(key) => match self.data.get_mut(key) {
                Some(Waiter::Occupied(w)) => {
                    if !w.will_wake(waker) {
                        *w = waker.clone()
                    }
                }
                _ => unreachable!("update non-existent waker"),
            },
            None => {
                let key = self.next;

                if self.data.len() == key {
                    self.data.push(Waiter::Occupied(waker.clone()));
                    self.next = key + 1;
                } else if let Some(&Waiter::Vacant(n)) = self.data.get(key) {
                    self.data[key] = Waiter::Occupied(waker.clone());
                    self.next = n;
                } else {
                    unreachable!();
                }

                *id = Some(key);
            }
        }
    }

    #[allow(dead_code)] // no remove case so far
    pub(crate) fn remove(&mut self, id: &mut Option<usize>) {
        if let Some(key) = id.take() {
            if let Some(waiter) = self.data.get_mut(key) {
                if let Waiter::Occupied(_) = waiter {
                    *waiter = Waiter::Vacant(self.next);
                    self.next = key;
                }
            }
        }
    }

    pub(crate) fn wake_all(mutex: &Mutex<Self>) {
        let waiters = {
            let mut lock = mutex.lock();
            lock.next = 0;
            take(&mut lock.data)
        };

        for waiter in waiters {
            if let Waiter::Occupied(w) = waiter {
                w.wake();
            }
        }
    }
}
