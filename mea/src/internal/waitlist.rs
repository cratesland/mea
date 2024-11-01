use slab::Slab;
use std::ptr;
use std::task::{Context, RawWaker, RawWakerVTable, Waker};

/// A guarded linked list.
///
/// * `guard`'s `next` points to the first node (regular head).
/// * `guard`'s `prev` points to the last node (regular tail).
#[derive(Debug)]
pub(crate) struct WaitList {
    guard: usize,
    nodes: Slab<Node>,
}

#[derive(Debug)]
struct Node {
    prev: usize,
    next: usize,
    waker: Waker,
}

impl WaitList {
    pub(crate) fn new() -> Self {
        let mut nodes = Slab::new();
        let first = nodes.vacant_entry();
        let guard = first.key();
        first.insert(Node {
            prev: guard,
            next: guard,
            waker: noop(),
        });
        Self { guard, nodes }
    }

    /// Registers a waker to the tail of the wait list.
    pub(crate) fn register_waker(&mut self, idx: &mut Option<usize>, cx: &mut Context<'_>) {
        match *idx {
            None => {
                let prev_tail = self.nodes[self.guard].prev;
                let new_node = Node {
                    prev: prev_tail,
                    next: self.guard,
                    waker: cx.waker().clone(),
                };
                let new_key = self.nodes.insert(new_node);
                self.nodes[self.guard].prev = new_key;
                self.nodes[prev_tail].next = new_key;
            }
            Some(key) => {
                debug_assert_ne!(key, self.guard);
                if !self.nodes[key].waker.will_wake(cx.waker()) {
                    self.nodes[key].waker = cx.waker().clone();
                }
            }
        }
    }

    /// Removes a previously registered waker from the wait list.
    pub(crate) fn remove_waker(&mut self, idx: usize) -> Node {
        debug_assert_ne!(idx, self.guard);
        let prev = self.nodes[idx].prev;
        let next = self.nodes[idx].next;
        self.nodes[prev].next = next;
        self.nodes[next].prev = prev;
        self.nodes.remove(idx)
    }

    /// Wakes up the first waiter in the list.
    pub(crate) fn wake_first(&mut self) {
        let first = self.nodes[self.guard].next;
        if first != self.guard {
            let first = self.remove_waker(first);
            first.waker.wake();
        }
    }
}

// TODO(tisonkun): Use the 'noop_waker' feature once it is stabilized
// @see https://github.com/rust-lang/rust/issues/98286
const fn noop() -> Waker {
    const NOOP: RawWaker = {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(|_| NOOP, |_| {}, |_| {}, |_| {});
        RawWaker::new(ptr::null(), &VTABLE)
    };
    unsafe { Waker::from_raw(NOOP) }
}
