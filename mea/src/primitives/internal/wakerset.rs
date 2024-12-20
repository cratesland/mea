use slab::Slab;
use std::task::{Context, Waker};

pub(crate) struct WakerSet {
    entries: Slab<Option<Waker>>,
    notifiable: usize,
}

impl WakerSet {
    /// Inserts a waker for a blocked operation and returns a key associated with it.
    pub fn insert(&mut self, cx: &Context<'_>) -> usize {
        let key = self.entries.insert(Some(cx.waker().clone()));
        self.notifiable += 1;
        key
    }

    /// If the waker for this key is still waiting for a notification, then update
    /// the waker for the entry, and return false. If the waker has been notified,
    /// treat the entry as completed and return true.
    pub fn remove_if_notified(&mut self, key: usize, cx: &Context<'_>) -> bool {
        match &mut self.entries[key] {
            None => {
                self.entries.remove(key);
                true
            }
            Some(w) => {
                // We were never woken, so update instead
                if !w.will_wake(cx.waker()) {
                    *w = cx.waker().clone();
                }
                false
            }
        }
    }

    /// Notifies all blocked operations.
    ///
    /// Returns `true` if at least one operation was notified.
    pub fn notify_all(&mut self) -> bool {
        if self.notifiable > 0 {
            let mut notified = false;
            for (_, opt_waker) in self.entries.iter_mut() {
                if let Some(w) = opt_waker.take() {
                    w.wake();
                    self.notifiable -= 1;
                    notified = true;
                }
            }
            assert_eq!(self.notifiable, 0);
            notified
        } else {
            false
        }
    }

    /// Notifies one additional blocked operation.
    ///
    /// Returns `true` if an operation was notified.
    pub fn notify_one(&mut self) -> bool {
        if self.notifiable > 0 {
            for (_, opt_waker) in self.entries.iter_mut() {
                if let Some(w) = opt_waker.take() {
                    w.wake();
                    self.notifiable -= 1;
                    return true;
                }
            }
        }
        false
    }

    /// Removes the waker of a cancelled operation.
    ///
    /// Returns `true` if another blocked operation from the set was notified.
    pub fn cancel(&mut self, key: usize) -> bool {
        match self.entries.remove(key) {
            Some(_) => {
                self.notifiable -= 1;
                false
            }
            // The operation was cancelled and notified so notify another operation instead.
            // If there is no waker in this entry, that means it was already woken.
            None => self.notify_one(),
        }
    }
}
