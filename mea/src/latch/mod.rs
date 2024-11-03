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

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use crate::internal::CountdownState;

#[cfg(test)]
mod tests;

/// A synchronization primitive that can be used to coordinate multiple tasks.
///
/// A latch starts with an initial count and tasks can wait for this count to reach zero.
/// The count can be decremented by calling [`count_down()`] or [`arrive()`]. Once the count
/// reaches zero, all waiting tasks are unblocked.
///
/// This is similar to Java's CountDownLatch but with async/await support.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use mea::latch::Latch;
///
/// async fn example() {
///     let latch = Arc::new(Latch::new(3));
///     let mut handles = Vec::new();
///
///     // Spawn tasks that will count down the latch
///     for i in 0..3 {
///         let latch = latch.clone();
///         handles.push(tokio::spawn(async move {
///             // Do some work
///             println!("Task {} completing", i);
///             latch.count_down();
///         }));
///     }
///
///     // Wait for all tasks to complete
///     latch.wait().await;
///     println!("All tasks completed!");
///
///     for handle in handles {
///         handle.await.unwrap();
///     }
/// }
/// ```
///
/// [`count_down()`]: Latch::count_down
/// [`arrive()`]: Latch::arrive
#[derive(Debug)]
pub struct Latch {
    state: CountdownState,
}

impl Latch {
    /// Creates a new latch initialized with the given count.
    ///
    /// # Arguments
    ///
    /// * `count` - The initial count value. Tasks will wait until this count reaches zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(3); // Creates a latch with count of 3
    /// ```
    pub fn new(count: u32) -> Self {
        Self {
            state: CountdownState::new(count),
        }
    }

    /// Returns the current count.
    ///
    /// This method is typically used for debugging and testing purposes.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(5);
    /// assert_eq!(latch.count(), 5);
    /// ```
    pub fn count(&self) -> u32 {
        self.state.state()
    }

    /// Decrements the latch count by one, waking up all pending tasks if the counter reaches zero.
    ///
    /// If the current count is zero, this method has no effect.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(2);
    /// latch.count_down(); // Count is now 1
    /// latch.count_down(); // Count is now 0, all waiting tasks are woken
    /// ```
    pub fn count_down(&self) {
        if self.state.decrement(1) {
            self.state.wake_all();
        }
    }

    /// Decrements the latch count by `n`, waking up all waiting tasks if the counter reaches zero.
    ///
    /// This method provides a way to decrement the counter by more than one at a time.
    /// It will not cause an overflow when decrementing the counter.
    ///
    /// # Arguments
    ///
    /// * `n` - The amount to decrement the counter by
    ///
    /// # Behavior
    ///
    /// * If `n` is zero or the counter has already reached zero, nothing happens
    /// * If the current count is greater than `n`, it is decremented by `n`
    /// * If the current count is greater than 0 but less than or equal to `n`, the count becomes
    ///   zero and all waiting tasks are woken
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(5);
    /// latch.arrive(3); // Count is now 2
    /// latch.arrive(2); // Count is now 0, all waiting tasks are woken
    /// ```
    pub fn arrive(&self, n: u32) {
        if n != 0 && self.state.decrement(n) {
            self.state.wake_all();
        }
    }

    /// Attempts to wait for the latch count to reach zero without blocking.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the count is zero
    /// * `Err(count)` if the count is not zero, where `count` is the current count
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::latch::Latch;
    ///
    /// let latch = Latch::new(2);
    /// assert_eq!(latch.try_wait(), Err(2));
    /// latch.count_down();
    /// assert_eq!(latch.try_wait(), Err(1));
    /// latch.count_down();
    /// assert_eq!(latch.try_wait(), Ok(()));
    /// ```
    pub fn try_wait(&self) -> Result<(), u32> {
        self.state.spin_wait(0)
    }

    /// Returns a future that will complete when the latch count reaches zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use mea::latch::Latch;
    ///
    /// async fn example() {
    ///     let latch = Arc::new(Latch::new(1));
    ///     let latch2 = latch.clone();
    ///
    ///     // Spawn a task that will wait for the latch
    ///     let handle = tokio::spawn(async move {
    ///         latch2.wait().await;
    ///         println!("Latch reached zero!");
    ///     });
    ///
    ///     // Count down the latch
    ///     latch.count_down();
    ///     handle.await.unwrap();
    /// }
    /// ```
    pub fn wait(&self) -> LatchWait<'_> {
        LatchWait {
            idx: None,
            latch: self,
        }
    }
}

/// A future returned by [`Latch::wait()`].
///
/// This future will complete when the latch count reaches zero.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct LatchWait<'a> {
    idx: Option<usize>,
    latch: &'a Latch,
}

impl fmt::Debug for LatchWait<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LatchWait").finish_non_exhaustive()
    }
}

impl Future for LatchWait<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { idx, latch } = self.get_mut();

        // register waker if the counter is not zero
        if latch.state.spin_wait(16).is_err() {
            latch.state.register_waker(idx, cx);
            // double check after register waker, to catch the update between two steps
            if latch.state.spin_wait(0).is_err() {
                return Poll::Pending;
            }
        }

        Poll::Ready(())
    }
}
