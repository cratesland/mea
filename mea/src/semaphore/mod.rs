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

use crate::internal;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct Semaphore {
    s: internal::Semaphore,
}

impl Semaphore {
    /// Constructs a `Semaphore` initialized with the given permits.
    pub fn new(permits: u32) -> Self {
        Self {
            s: internal::Semaphore::new(permits),
        }
    }

    /// Returns the current number of permits available in this semaphore.
    ///
    /// This method is typically used for debugging and testing purposes.
    pub fn available_permits(&self) -> u32 {
        self.s.available_permits()
    }

    /// Decrease a semaphore's permits by a maximum of `n`.
    ///
    /// If there are insufficient permits, and it's not possible to reduce by `n`,
    /// return the number of permits that were actually reduced.
    pub fn forget(&self, n: u32) -> u32 {
        self.s.forget(n)
    }

    /// Attempts to acquire `n` permits from this semaphore.
    pub fn try_acquire(&self, permits: u32) -> Option<SemaphorePermit<'_>> {
        self.s
            .try_acquire(permits)
            .then_some(SemaphorePermit { sem: self, permits })
    }

    /// Adds `n` new permits to the semaphore.
    ///
    /// # Panics
    ///
    /// This function panics if the semaphore would overflow.
    pub fn release(&self, permits: u32) {
        self.s.release(permits);
    }

    /// Acquires `n` permits from the semaphore.
    pub async fn acquire(&self, permits: u32) -> SemaphorePermit<'_> {
        self.s.acquire(permits).await;
        SemaphorePermit { sem: self, permits }
    }
}

/// A permit from the semaphore.
///
/// This type is created by the [`acquire`] method.
///
/// [`acquire`]: Semaphore::acquire()
#[must_use]
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    sem: &'a Semaphore,
    permits: u32,
}

impl SemaphorePermit<'_> {
    /// Forgets the permit **without** releasing it back to the semaphore.
    /// This can be used to reduce the amount of permits available from a
    /// semaphore.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Arc::new(Semaphore::new(10));
    /// {
    ///     let permit = sem.try_acquire(5).unwrap();
    ///     assert_eq!(sem.available_permits(), 5);
    ///     permit.forget();
    /// }
    ///
    /// // Since we forgot the permit, available permits won't go back to its initial value
    /// // even after the permit is dropped.
    /// assert_eq!(sem.available_permits(), 5);
    /// ```
    pub fn forget(mut self) {
        self.permits = 0;
    }

    /// Returns the number of permits held by `self`.
    pub fn permits(&self) -> u32 {
        self.permits
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.sem.release(self.permits);
    }
}
