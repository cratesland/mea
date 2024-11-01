use crate::internal::{Mutex, WaitList};
use std::sync::atomic::{AtomicU32, Ordering};

/// The internal semaphore that provides low-level async primitives.
#[derive(Debug)]
pub(crate) struct Semaphore {
    /// The current number of available permits in the semaphore.
    permits: AtomicU32,
    waiters: Mutex<WaitList>,
}

#[derive(Debug)]
pub(crate) struct Acquire<'a> {
    permits: usize,
    index: Option<usize>,
    semaphore: &'a Semaphore,
}

impl Semaphore {
    pub(crate) fn new(permits: u32) -> Self {
        Self {
            permits: AtomicU32::new(permits),
            waiters: Mutex::new(WaitList::new()),
        }
    }

    /// Returns the current number of available permits.
    pub(crate) fn available_permits(&self) -> u32 {
        self.permits.load(Ordering::Acquire)
    }

    /// Adds `n` new permits to the semaphore.
    pub(crate) fn release(&self, n: u32) {
        if n != 0 {

        }
    }

    fn do_release(&self, )
}
