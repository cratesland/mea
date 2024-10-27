use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering;
use core::task::Context;
use core::task::Waker;

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
    /// acquire (shared or exclusive) to succeed; and `false` otherwise.
    ///
    /// This method itself returns the result of `try_release_shared`.
    pub(crate) fn release_shared<F>(&self, arg: u32, try_release_shared: F) -> bool
    where
        F: FnOnce(&Self, u32) -> bool,
    {
        if try_release_shared(self, arg) {
            self.signal_next_node();
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
    /// `try_acquire_shared` returns false on failure; true if acquisition in shared mode succeeded.
    /// Upon success, this synchronizer has been acquired.
    pub(crate) fn acquire_shared<F>(
        &self,
        cx: &mut Context<'_>,
        arg: u32,
        mut try_acquire_shared: F,
    ) -> bool
    where
        F: FnMut(&Self, u32) -> bool,
    {
        if try_acquire_shared(self, arg) {
            self.signal_next_node();
            true
        } else {
            self.do_acquire(cx, arg, true, try_acquire_shared)
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
        let node = Node::new(cx.waker());

        // before the node start waiting, spin a while to try acquiring
        for _ in 0..16 {
            if try_acquire(self, arg) {
                node.waker.wake();
                if shared {
                    self.signal_next_node();
                }
                return true;
            }
            core::hint::spin_loop();
        }

        // enqueue a new waiter node
        self.waiters.push(node);
        false
    }

    fn signal_next_node(&self) {
        if let Some(node) = self.waiters.pop() {
            node.waker.wake();
        }
    }
}

#[derive(Debug)]
struct Node {
    waker: Waker,
}

impl Node {
    fn new(waker: &Waker) -> Self {
        Self {
            waker: waker.clone(),
        }
    }
}
