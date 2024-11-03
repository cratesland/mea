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

/// An async counting semaphore for controlling access to a set of resources.
///
/// A semaphore maintains a set of permits. Permits are used to synchronize access
/// to a pool of resources. Each [`acquire`] call blocks until a permit is available,
/// and then takes one permit. Each [`release`] call adds a new permit, potentially
/// releasing a blocked acquirer.
///
/// Semaphores are often used to restrict the number of tasks that can access some
/// (physical or logical) resource. For example, here is a class that uses a
/// semaphore to control access to a pool of connections:
///
/// ```
/// use std::sync::Arc;
///
/// use mea::semaphore::Semaphore;
///
/// struct Pool {
///     sem: Arc<Semaphore>,
///     // ... other fields for managing connections
/// }
///
/// impl Pool {
///     fn new(size: u32) -> Self {
///         Self {
///             sem: Arc::new(Semaphore::new(size)),
///             // ... initialize other fields
///         }
///     }
///
///     async fn get_connection(&self) -> Connection {
///         let _permit = self.sem.acquire(1).await;
///         // ... acquire and return a connection
///         Connection {}
///     }
/// }
///
/// // Dummy connection type for the example
/// struct Connection {}
/// ```
///
/// [`acquire`]: Semaphore::acquire
/// [`release`]: Semaphore::release
#[derive(Debug)]
pub struct Semaphore {
    s: internal::Semaphore,
}

impl Semaphore {
    /// Creates a new semaphore with the given number of permits.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Semaphore::new(5); // Creates a semaphore with 5 permits
    /// ```
    pub fn new(permits: u32) -> Self {
        Self {
            s: internal::Semaphore::new(permits),
        }
    }

    /// Returns the current number of permits available.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Semaphore::new(2);
    /// assert_eq!(sem.available_permits(), 2);
    ///
    /// let permit = sem.try_acquire(1).unwrap();
    /// assert_eq!(sem.available_permits(), 1);
    /// ```
    pub fn available_permits(&self) -> u32 {
        self.s.available_permits()
    }

    /// Reduces the semaphore's permits by a maximum of `n`.
    ///
    /// Returns the actual number of permits that were reduced. This may be less
    /// than `n` if there are insufficient permits available.
    ///
    /// This is useful when you want to permanently remove permits from the semaphore.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Semaphore::new(5);
    /// assert_eq!(sem.forget(3), 3); // Removes 3 permits
    /// assert_eq!(sem.available_permits(), 2);
    ///
    /// // Trying to forget more permits than available
    /// assert_eq!(sem.forget(3), 2); // Only removes remaining 2 permits
    /// assert_eq!(sem.available_permits(), 0);
    /// ```
    pub fn forget(&self, n: u32) -> u32 {
        self.s.forget(n)
    }

    /// Attempts to acquire `n` permits from the semaphore without blocking.
    ///
    /// If the permits are successfully acquired, a [`SemaphorePermit`] is returned.
    /// The permits will be automatically returned to the semaphore when the permit
    /// is dropped, unless [`forget`] is called.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Semaphore::new(2);
    ///
    /// // First acquisition succeeds
    /// let permit1 = sem.try_acquire(1).unwrap();
    /// assert_eq!(sem.available_permits(), 1);
    ///
    /// // Second acquisition succeeds
    /// let permit2 = sem.try_acquire(1).unwrap();
    /// assert_eq!(sem.available_permits(), 0);
    ///
    /// // Third acquisition fails
    /// assert!(sem.try_acquire(1).is_none());
    /// ```
    ///
    /// [`forget`]: SemaphorePermit::forget
    pub fn try_acquire(&self, permits: u32) -> Option<SemaphorePermit<'_>> {
        self.s
            .try_acquire(permits)
            .then_some(SemaphorePermit { sem: self, permits })
    }

    /// Adds `n` new permits to the semaphore.
    ///
    /// # Panics
    ///
    /// Panics if adding the permits would cause the total number of permits to overflow.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Semaphore::new(0);
    /// sem.release(2); // Adds 2 permits
    /// assert_eq!(sem.available_permits(), 2);
    /// ```
    pub fn release(&self, permits: u32) {
        self.s.release(permits);
    }

    /// Acquires `n` permits from the semaphore.
    ///
    /// If the permits are not immediately available, this method will wait until
    /// they become available. Returns a [`SemaphorePermit`] that will release the
    /// permits when dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use mea::semaphore::Semaphore;
    ///
    /// async fn example() {
    ///     let sem = Arc::new(Semaphore::new(2));
    ///     let sem2 = sem.clone();
    ///
    ///     let handle = tokio::spawn(async move {
    ///         let permit = sem2.acquire(1).await;
    ///         // Do some work with the permit
    ///         // Permit is automatically released when dropped
    ///     });
    ///
    ///     let permit = sem.acquire(1).await;
    ///     // Do some work with the permit
    ///     drop(permit); // Explicitly release the permit
    ///
    ///     handle.await.unwrap();
    /// }
    /// ```
    pub async fn acquire(&self, permits: u32) -> SemaphorePermit<'_> {
        self.s.acquire(permits).await;
        SemaphorePermit { sem: self, permits }
    }
}

/// A permit from the semaphore.
///
/// This type is created by the [`acquire`] and [`try_acquire`] methods on [`Semaphore`].
/// When the permit is dropped, the permits will be returned to the semaphore unless
/// [`forget`] is called.
///
/// [`acquire`]: Semaphore::acquire
/// [`try_acquire`]: Semaphore::try_acquire
/// [`forget`]: SemaphorePermit::forget
#[must_use = "permits are released immediately when dropped"]
#[derive(Debug)]
pub struct SemaphorePermit<'a> {
    sem: &'a Semaphore,
    permits: u32,
}

impl SemaphorePermit<'_> {
    /// Forgets the permit **without** releasing it back to the semaphore.
    ///
    /// This can be used to permanently reduce the number of permits available
    /// from a semaphore.
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
    /// // Since we forgot the permit, available permits won't go back to
    /// // its initial value even after the permit is dropped
    /// assert_eq!(sem.available_permits(), 5);
    /// ```
    pub fn forget(mut self) {
        self.permits = 0;
    }

    /// Returns the number of permits this permit holds.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::semaphore::Semaphore;
    ///
    /// let sem = Semaphore::new(5);
    /// let permit = sem.try_acquire(3).unwrap();
    /// assert_eq!(permit.permits(), 3);
    /// ```
    pub fn permits(&self) -> u32 {
        self.permits
    }
}

impl Drop for SemaphorePermit<'_> {
    fn drop(&mut self) {
        self.sem.release(self.permits);
    }
}