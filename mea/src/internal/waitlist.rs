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

/// A guarded linked list.
///
/// * `guard`'s `next` points to the first node (regular head).
/// * `guard`'s `prev` points to the last node (regular tail).
#[derive(Debug)]
pub(crate) struct WaitList<T> {
    guard: usize,
    nodes: Slab<Node<T>>,
}

#[derive(Debug)]
struct Node<T> {
    prev: usize,
    next: usize,
    stat: Option<T>,
}

impl<T> WaitList<T> {
    pub(crate) fn new() -> Self {
        let mut nodes = Slab::new();
        let first = nodes.vacant_entry();
        let guard = first.key();
        first.insert(Node {
            prev: guard,
            next: guard,
            stat: None,
        });
        Self { guard, nodes }
    }

    /// Registers a waiter to the head of the wait list.
    ///
    /// # Panic
    ///
    /// Panics if `idx` is `Some`.
    pub(crate) fn registry_waiter_to_head(
        &mut self,
        idx: &mut Option<usize>,
        f: impl FnOnce() -> Option<T>,
    ) {
        assert!(idx.is_none());

        let stat = f();
        let prev_head = self.nodes[self.guard].next;
        let new_node = Node {
            prev: self.guard,
            next: prev_head,
            stat,
        };
        let new_key = self.nodes.insert(new_node);
        self.nodes[self.guard].next = new_key;
        self.nodes[prev_head].prev = new_key;
        *idx = Some(new_key);
    }

    /// Registers a waiter to the tail of the wait list.
    ///
    /// # Panic
    ///
    /// Panics if `idx` is `Some`.
    pub(crate) fn register_waiter_to_tail(
        &mut self,
        idx: &mut Option<usize>,
        f: impl FnOnce() -> Option<T>,
    ) {
        assert!(idx.is_none());

        let stat = f();
        let prev_tail = self.nodes[self.guard].prev;
        let new_node = Node {
            prev: prev_tail,
            next: self.guard,
            stat,
        };
        let new_key = self.nodes.insert(new_node);
        self.nodes[self.guard].prev = new_key;
        self.nodes[prev_tail].next = new_key;
        *idx = Some(new_key);
    }

    /// Removes a previously registered waker from the wait list.
    pub(crate) fn remove_waiter(
        &mut self,
        idx: usize,
        f: impl FnOnce(&mut T) -> bool,
    ) -> Option<&mut T> {
        assert_ne!(idx, self.guard);

        // SAFETY: `idx` is a valid key + non-guard node always has `Some(stat)`
        fn retrieve_stat<T>(node: &mut Node<T>) -> &mut T {
            node.stat.as_mut().unwrap()
        }

        if f(retrieve_stat(&mut self.nodes[idx])) {
            let prev = self.nodes[idx].prev;
            let next = self.nodes[idx].next;
            self.nodes[prev].next = next;
            self.nodes[next].prev = prev;
            Some(retrieve_stat(&mut self.nodes[idx]))
        } else {
            None
        }
    }

    /// Removes the first waiter from the wait list.
    pub(crate) fn remove_first_waiter(&mut self, f: impl FnOnce(&mut T) -> bool) -> Option<&mut T> {
        let first = self.nodes[self.guard].next;
        if first != self.guard {
            self.remove_waiter(first, f)
        } else {
            None
        }
    }

    /// Returns `true` if the wait list is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes[self.guard].next == self.guard
    }

    pub(crate) fn with_mut(&mut self, idx: usize, drop: impl FnOnce(&mut T) -> bool) {
        let node = &mut self.nodes[idx];
        if drop(node.stat.as_mut().unwrap()) {
            self.nodes.remove(idx);
        }
    }
}
