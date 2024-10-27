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

use crate::internal::WaitQueueSync;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

#[cfg(test)]
mod tests;

pub struct Semaphore {
    sync: WaitQueueSync,
}

impl fmt::Debug for Semaphore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Semaphore")
            .field("permits", &self.available_permits())
            .finish_non_exhaustive()
    }
}

impl Semaphore {
    /// Constructs a `Semaphore` initialized with the given permits.
    pub const fn new(permits: u32) -> Self {
        Self {
            sync: WaitQueueSync::new(permits),
        }
    }

    /// Returns the current number of permits available in this semaphore.
    ///
    /// This method is typically used for debugging and testing purposes.
    pub fn available_permits(&self) -> usize {
        self.sync.state() as usize
    }

    /// Attempts to acquire `n` permits from this semaphore.
    pub fn try_acquire(&self, permits: u32) -> bool {
        non_fair_try_acquire_shared(&self.sync, permits).is_some()
    }

    /// Adds `n` new permits to the semaphore.
    ///
    /// # Panics
    ///
    /// This function panics if the semaphore would overflow.
    pub fn release(&self, permits: u32) {
        self.sync.release_shared(permits, |sync, n| {
            let mut current = sync.state();
            loop {
                let next = current.checked_add(n).expect("permits overflow");
                match sync.cas_state(current, next) {
                    Ok(_) => return true,
                    Err(x) => current = x,
                }
            }
        });
    }

    /// Acquires `n` permits from the semaphore.
    pub fn acquire(&self, permits: u32) -> Acquire {
        Acquire {
            semaphore: self,
            permits,
        }
    }
}

fn non_fair_try_acquire_shared(sync: &WaitQueueSync, acquires: u32) -> Option<()> {
    let mut available = sync.state();
    loop {
        let remaining = available.checked_sub(acquires)?;
        match sync.cas_state(available, remaining) {
            Ok(_) => return Some(()),
            Err(x) => available = x,
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Acquire<'a> {
    semaphore: &'a Semaphore,
    permits: u32,
}

impl fmt::Debug for Acquire<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Acquire")
            .field("permits", &self.permits)
            .finish_non_exhaustive()
    }
}

impl Future for Acquire<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { semaphore, permits } = self.get_mut();
        if semaphore
            .sync
            .acquire_shared(cx, *permits, non_fair_try_acquire_shared)
            .is_some()
        {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
