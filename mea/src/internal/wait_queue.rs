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

use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;
use core::task::Context;

use crossbeam_queue::SegQueue;

#[derive(Debug)]
pub(crate) struct WaitQueueSync {
    state: AtomicU32,
    waiters: SegQueue<Node>,
}

impl WaitQueueSync {
    pub(crate) const fn new(count: u32) -> Self {
        Self {
            state: AtomicU32::new(count),
            waiters: SegQueue::new(),
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

    /// Releases in shared mode. Implemented by dequeue and wake up one or more nodes if
    /// `try_release_shared` returns `true`.
    ///
    /// `arg` is passed to `try_release_shared` but is otherwise uninterpreted and can represent
    /// anything on demand.
    ///
    /// `try_release_shared` returns `true` if this release of shared mode may permit a waiting
    /// acquire to succeed; and `false` otherwise.
    pub(crate) fn release_shared<T, F>(&self, arg: T, try_release_shared: F)
    where
        F: FnOnce(&Self, T) -> bool,
    {
        if try_release_shared(self, arg) {
            self.signal_next_node();
        }
    }

    /// Acquires in shared mode. Implemented by invoking at least once `try_acquire_shared`,
    /// returning `Some` on success. Otherwise, return `None` to indicate a [`Poll::Pending`]
    /// in the caller side.
    ///
    /// `arg` is passed to `try_acquire_shared` but is otherwise uninterpreted and can represent
    /// anything on demand. Note that `arg` may be **copied** before passed to `try_acquire_shared`.
    ///
    /// `try_acquire_shared` returns `None` on failure; `Some` if acquisition in shared mode
    /// succeeded. Upon success, this synchronizer has been acquired.
    pub(crate) fn acquire_shared<F, T, R>(
        &self,
        cx: &mut Context<'_>,
        arg: T,
        mut try_acquire_shared: F,
    ) -> Option<R>
    where
        T: Copy,
        F: FnMut(&Self, T) -> Option<R>,
    {
        match try_acquire_shared(self, arg) {
            Some(result) => {
                self.signal_next_node();
                Some(result)
            }
            None => self.do_acquire(cx, arg, true, try_acquire_shared),
        }
    }

    /// Main acquire method. Returns `Some` if acquired; `None` otherwise.
    fn do_acquire<F, T, R>(
        &self,
        cx: &mut Context<'_>,
        arg: T,
        shared: bool,
        mut try_acquire: F,
    ) -> Option<R>
    where
        T: Copy,
        F: FnMut(&Self, T) -> Option<R>,
    {
        // 1. before the node start waiting, spin a while to try acquiring
        for _ in 0..16 {
            if let Some(result) = try_acquire(self, arg) {
                if shared {
                    self.signal_next_node();
                }
                return Some(result);
            }
            core::hint::spin_loop();
        }

        // 2. enqueue a new waiter node
        // TODO(tisonkun): if the outer future store Arc<Waker>, it may be possible to check
        //  `will_wake` here to avoid the push; it would work like if the wakers are stored in
        //  an mutex guarded slab waiter set and the outer future hold the id, as this crate
        //  previously did; review whether it is desired later. (Arc::try_unwrap & Arc::ptr_eq)
        let node = Node::new(cx.waker());
        self.waiters.push(node);

        // 3. check again after enqueuing, avoid forever waiting
        if let Some(result) = try_acquire(self, arg) {
            if let Some(node) = self.waiters.pop() {
                // TODO(tisonkun): make it simple for now, review whether it is desired later
                // @see 6f740fddb6ae64ea993dacec12b0cbe75b64e9ce for a possible revert
                // // the current call to `poll` is about to return ready, so need not wake up
                // // the node just enqueued
                // if token != node.token() {
                //     node.wake();
                // }
                node.wake();
            }
            return Some(result);
        }

        None
    }

    fn signal_next_node(&self) {
        if let Some(node) = self.waiters.pop() {
            node.wake();
        }
    }
}

use node::*;
// encapsulate node fields
mod node {
    use core::task::Waker;

    #[derive(Debug)]
    pub(super) struct Node {
        waker: Waker,
    }
    impl Node {
        pub(super) fn new(waker: &Waker) -> Self {
            Self {
                waker: waker.clone(),
            }
        }

        pub(super) fn wake(self) {
            self.waker.wake();
        }
    }
}

// common utilities
mod utils {
    use super::*;

    impl WaitQueueSync {
        pub(crate) fn release_shared_by_one(&self) {
            self.release_shared_by_n(1)
        }

        pub(crate) fn release_shared_by_n(&self, n: u32) {
            self.release_shared(n, |sync, n| {
                let mut cnt = sync.state();
                loop {
                    if cnt == 0 {
                        return false;
                    }
                    let new_cnt = cnt.saturating_sub(n);
                    match sync.cas_state(cnt, new_cnt) {
                        Ok(_) => return new_cnt == 0,
                        Err(x) => cnt = x,
                    }
                }
            });
        }

        /// Returns `true` if the current state is zero; otherwise, add the waker to the wait queue
        /// and returns `false`.
        pub(crate) fn acquire_shared_on_state_is_zero(&self, cx: &mut Context<'_>) -> bool {
            let result = self.acquire_shared(cx, 1, |sync, _| (sync.state() == 0).then_some(()));
            result.is_some()
        }
    }
}
