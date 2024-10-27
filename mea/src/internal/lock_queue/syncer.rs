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

use alloc::boxed::Box;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;
use core::task::Context;

use crate::internal::lock_queue::node::Node;
use crate::internal::lock_queue::ptr_to_node;

// Node status bits, also used as argument and return values
const WAITING: i8 = 1;

/// A waker queue implements a variant of a CLH lock queue.
pub(crate) struct LockQueueSyncer {
    head: AtomicPtr<Node>,
    tail: AtomicPtr<Node>,
    // TODO(tisonkun): whether to use macro or trait abstraction to support any atomic integer
    state: AtomicU32,
}

impl Drop for LockQueueSyncer {
    fn drop(&mut self) {}
}

impl LockQueueSyncer {
    pub(crate) const fn new(state: u32) -> Self {
        LockQueueSyncer {
            head: AtomicPtr::new(ptr::null_mut()),
            tail: AtomicPtr::new(ptr::null_mut()),
            state: AtomicU32::new(state),
        }
    }

    /// Performs volatile read on `state`.
    ///
    /// All other writes to `state` should be at least [`Ordering::Release`].
    pub(crate) fn state(&self) -> u32 {
        self.state.load(Ordering::Acquire)
    }

    /// Performs volatile CAS on `state.
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
    /// acquire (shared or exclusive) to succeed; ; and `false` otherwise.
    ///
    /// This method itself returns the result of `try_release_shared`.
    pub(crate) fn release_shared<F>(&self, arg: u32, try_release_shared: F) -> bool
    where
        F: FnOnce(&Self, u32) -> bool,
    {
        if try_release_shared(self, arg) {
            if let Some(head) = ptr_to_node(&self.head) {
                signal_next_node(head);
            }
            true
        } else {
            false
        }
    }

    /// Acquires in shared mode. Implemented by invoking at least once `try_acquire_shared`,
    /// returning `true` on success. Otherwise, return `false` to indicate a [`Poll::Pending`]
    /// in the caller side.
    ///
    /// `arg` is passed to `try_acquire_shared` but is otherwise uninterpreted and can represent
    /// anything on demand.
    ///
    /// `try_acquire_shared` returns a negative value on failure; zero if acquisition in shared mode
    /// succeeded but no subsequent shared-mode acquire can succeed; and a positive value if
    /// acquisition in shared mode succeeded and subsequent shared-mode acquires might also succeed,
    /// in which case a subsequent waiting waker must check availability. (Support for three
    /// different return values enables this method to be used in contexts where acquires only
    /// sometimes act exclusively.) Upon success, this syncer has been acquired.
    pub(crate) fn acquire_shared<F>(
        &self,
        cx: &mut Context<'_>,
        arg: u32,
        mut try_acquire_shared: F,
    ) -> bool
    where
        F: FnMut(&Self, u32) -> i8,
    {
        if try_acquire_shared(self, arg) >= 0 {
            true
        } else {
            self.do_acquire(cx, arg, true, |syncer, arg| {
                try_acquire_shared(syncer, arg) >= 0
            })
        }
    }

    /// Main acquire method. Returns true if acquired; false otherwise.
    fn do_acquire<F>(
        &self,
        cx: &mut Context<'_>,
        arg: u32,
        shared: bool,
        mut try_acquire: F,
    ) -> bool
    where
        // returns true if acquired; false otherwise
        F: FnMut(&Self, u32) -> bool,
    {
        // spin a bit to retry before return pending, if this is the first node in the wait queue
        let (mut first, mut enqueued, mut spins) = (false, false, 16u8);

        let mut node = None::<NonNull<Node>>;
        loop {
            // check if node now first
            if !first {
                let prev = node
                    .as_ref()
                    .and_then(|node| unsafe { node.as_ref() }.prev());
                if let Some(prev) = prev {
                    enqueued = true;
                    let head = self.head.load(Ordering::Acquire);
                    first = ptr::eq(head, prev);
                    if !first && prev.prev().is_none() {
                        // ensure serialization
                        core::hint::spin_loop();
                        continue;
                    }
                }
            }

            // if node not yet enqueued, try acquiring
            if !enqueued && try_acquire(self, arg) {
                // drop the current node
                node.map(|p| unsafe { Box::from_raw(p.as_ptr()) });
                return true;
            }

            // if node is first, try acquiring
            if first && try_acquire(self, arg) {
                let mut node = match node.take() {
                    Some(node) => node,
                    None => unreachable!("node must be initialized as the first node"),
                };
                let node_ref = unsafe { node.as_mut() };
                let prev = node_ref.prev_ptr();
                node_ref.unset_waker();
                node_ref.set_prev(ptr::null_mut());
                self.head.store(node.as_ptr(), Ordering::Release);

                // drop the head node
                unsafe { ptr::drop_in_place(prev) };
                if shared {
                    signal_next_node_if_shared(node_ref);
                }
                return true;
            }

            let tail = match ptr_to_node(&self.tail) {
                Some(tail) => tail,
                None =>
                // initialize the queue
                {
                    self.do_initialize_head();
                    continue;
                }
            };

            let (node_ref, node_raw) = match node {
                Some(mut node) => (unsafe { node.as_mut() }, node.as_ptr()),
                None =>
                // allocate; retry before enqueue
                {
                    let state = Node::new(shared, cx.waker());
                    node = NonNull::new(Box::into_raw(Box::new(state)));
                    continue;
                }
            };

            if !enqueued
            // try to enqueue
            {
                let tail_raw = tail as *mut Node;
                node_ref.set_waker(cx.waker());
                node_ref.set_prev_relaxed(tail_raw); // avoid unnecessary fence
                if self.cas_tail(tail_raw, node_raw) {
                    tail.set_next(node_raw);
                } else {
                    // back out
                    node_ref.set_prev_relaxed(ptr::null_mut());
                }
                continue;
            }

            if first && spins > 0 {
                spins -= 1;
                core::hint::spin_loop();
                continue;
            }

            if node_ref.status() == 0
            // enable signal and recheck
            {
                node_ref.set_status(WAITING);
                continue;
            }

            return false;
        }
    }

    fn do_initialize_head<'a>(&self) -> &'a mut Node {
        let mut new_head = ptr::null_mut::<Node>();

        loop {
            if let Some(tail) = ptr_to_node(&self.tail) {
                return tail;
            }

            if ptr_to_node(&self.head).is_some() {
                // other thread has initialized the queue; busy wait since it should succeed soon
                core::hint::spin_loop();
                continue;
            }

            if new_head.is_null() {
                new_head = Box::into_raw(Box::new(Node::new_head()));
            }

            if self.cas_head(ptr::null_mut(), new_head) {
                self.tail.store(new_head, Ordering::Release);
                return unsafe { &mut *new_head };
            }
        }
    }

    // TODO(tisonkun): try to relax the memory order requirement for cas_head and cas_tail
    fn cas_head(&self, current: *mut Node, new: *mut Node) -> bool {
        self.head
            .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    fn cas_tail(&self, current: *mut Node, new: *mut Node) -> bool {
        self.tail
            .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

/// Wakes up the successor of `node`, if one exists, and unsets its WAITING status to avoid race.
// The return value is ignored; only for simplify null pointer handling.
fn signal_next_node(node: &Node) {
    let Some(next) = node.next() else {
        return;
    };

    if next.status() != 0 {
        next.fetch_unset_status(WAITING);
        next.wake();
    }
}

/// Wakes up the successor of `node`, if one exists, and unsets its WAITING status to avoid race.
// The return value is ignored; only for simplify null pointer handling.
fn signal_next_node_if_shared(node: &Node) {
    let Some(next) = node.next() else {
        return;
    };

    if next.is_shared() && next.status() != 0 {
        next.fetch_unset_status(WAITING);
        next.wake();
    }
}
