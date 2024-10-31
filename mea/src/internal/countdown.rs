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

use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::task::Context;
use std::task::Waker;

use slab::Slab;

#[derive(Debug)]
pub(crate) struct CountdownState {
    state: AtomicU32,
    waiters: Mutex<Slab<Waker>>,
}

impl CountdownState {
    pub(crate) const fn new(count: u32) -> Self {
        Self {
            state: AtomicU32::new(count),
            waiters: Mutex::new(Slab::new()),
        }
    }

    /// Performs volatile read on `state`.
    ///
    /// All other writes to `state` should be at least [`Ordering::Release`].
    pub(crate) fn state(&self) -> u32 {
        self.state.load(Ordering::Acquire)
    }

    /// Performs volatile CAS on `state`.
    ///
    /// If the comparison succeeds, performs read-modify-write operation with [`Ordering::Relaxed`]
    /// for read, and [`Ordering::Release`] for write; if the comparison fails, performs load
    /// operation with [`Ordering::Relaxed`].
    ///
    /// @see https://doc.rust-lang.org/std/sync/atomic/struct.AtomicU32.html#method.compare_exchange_weak
    /// @see https://en.cppreference.com/w/cpp/atomic/atomic_compare_exchange
    pub(crate) fn cas_state(&self, current: u32, new: u32) -> Result<(), u32> {
        self.state
            .compare_exchange_weak(current, new, Ordering::Release, Ordering::Relaxed)
            .map(|_| ())
    }

    /// Drain and wake up all waiters.
    pub(crate) fn wake_all(&self) {
        let mut waiters = self.waiters.lock().unwrap();
        if !waiters.is_empty() {
            for waker in waiters.drain() {
                waker.wake();
            }
        }
    }

    /// Registers a waker to be woken up when the countdown reaches zero.
    ///
    /// `idx` must be `None` when the waker is not registered, or `Some(key)` where `key` is
    /// a value previously returned by this method.
    pub(crate) fn register_waker(&self, idx: &mut Option<usize>, cx: &mut Context<'_>) {
        let mut waiters = self.waiters.lock().unwrap();
        match *idx {
            None => {
                let key = waiters.insert(cx.waker().clone());
                *idx = Some(key);
            }
            Some(key) => {
                if !waiters[key].will_wake(cx.waker()) {
                    waiters[key] = cx.waker().clone();
                }
            }
        }
    }

    /// Returns `Ok(())` if the counter is zero, otherwise returns `Err(s)` where `s` is the current
    /// counter value.
    pub(crate) fn spin_wait(&self, n: usize) -> Result<(), u32> {
        for _ in 0..n {
            if self.state() == 0 {
                return Ok(());
            }
            std::hint::spin_loop();
        }

        match self.state() {
            0 => Ok(()),
            s => Err(s),
        }
    }

    /// Decrements the counter, and returns whether the caller should wake up all waiters.
    pub(crate) fn decrement(&self, n: u32) -> bool {
        let mut cnt = self.state();
        loop {
            if cnt == 0 {
                // the one who decrements the counter to zero should wake up all waiters, not this
                // one
                return false;
            }

            let new_cnt = cnt.saturating_sub(n);
            match self.cas_state(cnt, new_cnt) {
                Ok(_) => return new_cnt == 0,
                Err(x) => cnt = x,
            }
        }
    }
}
