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

use core::ptr;
use core::sync::atomic::AtomicI8;
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;
use core::task::RawWaker;
use core::task::RawWakerVTable;
use core::task::Waker;

use crate::internal::lock_queue::ptr_to_node;

#[derive(Debug)]
pub(super) enum Node {
    Exclusive(NodeState),
    Shared(NodeState),
}

// manually dispatch methods
impl Node {
    pub(super) fn new(shared: bool, waker: &Waker) -> Self {
        let node_state = NodeState {
            prev: AtomicPtr::new(ptr::null_mut()),
            next: AtomicPtr::new(ptr::null_mut()),
            status: AtomicI8::new(0),
            waker: waker.clone(),
        };
        if shared {
            Node::Shared(node_state)
        } else {
            Node::Exclusive(node_state)
        }
    }

    // CLH queues need a dummy header node to get started.
    pub(super) fn new_head() -> Self {
        Node::new(false, &noop_waker())
    }

    pub(super) fn is_shared(&self) -> bool {
        match self {
            Node::Exclusive(_) => false,
            Node::Shared(_) => true,
        }
    }

    pub(super) fn set_next(&self, next: *mut Node) {
        match self {
            Node::Exclusive(node) => node.next.store(next, Ordering::Release),
            Node::Shared(node) => node.next.store(next, Ordering::Release),
        }
    }

    pub(super) fn next<'a>(&self) -> Option<&'a mut Node> {
        ptr_to_node(match self {
            Node::Exclusive(node) => &node.next,
            Node::Shared(node) => &node.next,
        })
    }

    pub(super) fn fetch_unset_status(&self, v: i8) -> i8 {
        match self {
            Node::Exclusive(node) => node.fetch_unset_status(v),
            Node::Shared(node) => node.fetch_unset_status(v),
        }
    }

    pub(super) fn set_status(&self, v: i8) {
        match self {
            Node::Exclusive(node) => node.status.store(v, Ordering::Release),
            Node::Shared(node) => node.status.store(v, Ordering::Release),
        }
    }

    pub(super) fn status(&self) -> i8 {
        match self {
            Node::Exclusive(node) => node.status.load(Ordering::Acquire),
            Node::Shared(node) => node.status.load(Ordering::Acquire),
        }
    }

    pub(super) fn unset_waker(&mut self) {
        match self {
            Node::Exclusive(node) => node.waker = noop_waker(),
            Node::Shared(node) => node.waker = noop_waker(),
        }
    }

    pub(super) fn set_waker(&mut self, waker: &Waker) {
        match self {
            Node::Exclusive(node) => {
                if !node.waker.will_wake(waker) {
                    node.waker = waker.clone();
                }
            }
            Node::Shared(node) => {
                if !node.waker.will_wake(waker) {
                    node.waker = waker.clone();
                }
            }
        }
    }

    pub(super) fn wake(&self) {
        match self {
            Node::Exclusive(node) => node.waker.wake_by_ref(),
            Node::Shared(node) => node.waker.wake_by_ref(),
        }
    }

    pub(super) fn set_prev_relaxed(&self, prev: *mut Node) {
        match self {
            Node::Exclusive(node) => node.prev.store(prev, Ordering::Relaxed),
            Node::Shared(node) => node.prev.store(prev, Ordering::Relaxed),
        }
    }

    pub(super) fn set_prev(&self, prev: *mut Node) {
        match self {
            Node::Exclusive(node) => node.prev.store(prev, Ordering::Release),
            Node::Shared(node) => node.prev.store(prev, Ordering::Release),
        }
    }

    pub(super) fn prev_ptr(&self) -> *mut Node {
        match self {
            Node::Exclusive(node) => node.prev.load(Ordering::Acquire),
            Node::Shared(node) => node.prev.load(Ordering::Acquire),
        }
    }

    pub(super) fn prev<'a>(&self) -> Option<&'a mut Node> {
        ptr_to_node(match self {
            Node::Exclusive(node) => &node.prev,
            Node::Shared(node) => &node.prev,
        })
    }
}

#[derive(Debug)]
pub(super) struct NodeState {
    prev: AtomicPtr<Node>,
    next: AtomicPtr<Node>,
    // currently, only NORMAL(0) and WAITING(1)
    status: AtomicI8,
    waker: Waker,
}

impl NodeState {
    /// Atomically replaces the current value of `status` with the result of bitwise AND between the
    /// current value and `!v`. Returns the previous value.
    fn fetch_unset_status(&self, v: i8) -> i8 {
        self.status.fetch_and(!v, Ordering::AcqRel)
    }
}

// TODO(tisonkun): use `Waker::noop` once https://github.com/rust-lang/rust/issues/98286 is stabilized
fn noop_waker() -> Waker {
    const NOOP: RawWaker = {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            // Cloning just returns a new no-op raw waker
            |_| NOOP,
            // `wake` does nothing
            |_| {},
            // `wake_by_ref` does nothing
            |_| {},
            // Dropping does nothing as we don't allocate anything
            |_| {},
        );
        RawWaker::new(ptr::null(), &VTABLE)
    };

    unsafe { Waker::from_raw(NOOP) }
}
