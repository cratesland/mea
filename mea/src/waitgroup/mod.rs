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
use std::future::IntoFuture;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use crate::internal::CountdownState;

#[cfg(test)]
mod tests;

/// A synchronization primitive that allows waiting for multiple tasks to complete.
///
/// A WaitGroup waits for a collection of tasks to finish. The main task calls
/// [`clone()`] to create a new worker handle for each task, and can then wait
/// for all tasks to complete by calling `.await` on the WaitGroup.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use mea::waitgroup::WaitGroup;
///
/// async fn example() {
///     let wg = WaitGroup::new();
///     let mut handles = Vec::new();
///
///     // Spawn multiple tasks
///     for i in 0..3 {
///         let wg = wg.clone(); // Create a new worker handle
///         handles.push(tokio::spawn(async move {
///             // Simulate some work
///             tokio::time::sleep(Duration::from_millis(100)).await;
///             println!("Task {} completed", i);
///             // Wait until all tasks have finished
///             wg.await
///         }));
///     }
///
///     // Wait for all tasks to complete
///     wg.await;
///
///     // All tasks have finished
///     println!("All tasks completed");
///
///     for handle in handles {
///         handle.await.unwrap();
///     }
/// }
/// ```
///
/// [`clone()`]: WaitGroup::clone
#[derive(Debug)]
pub struct WaitGroup {
    state: Arc<CountdownState>,
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitGroup {
    /// Creates a new `WaitGroup`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mea::waitgroup::WaitGroup;
    ///
    /// let wg = WaitGroup::new();
    /// ```
    pub fn new() -> Self {
        Self {
            state: Arc::new(CountdownState::new(1)),
        }
    }
}

impl Clone for WaitGroup {
    /// Creates a new worker handle for the WaitGroup.
    ///
    /// This increments the WaitGroup counter. The counter will be decremented
    /// when the new handle is dropped.
    fn clone(&self) -> Self {
        let sync = self.state.clone();
        let mut cnt = sync.state();
        loop {
            let new_cnt = cnt.saturating_add(1);
            match sync.cas_state(cnt, new_cnt) {
                Ok(_) => return Self { state: sync },
                Err(x) => cnt = x,
            }
        }
    }
}

impl Drop for WaitGroup {
    fn drop(&mut self) {
        if self.state.decrement(1) {
            self.state.wake_all();
        }
    }
}

impl IntoFuture for WaitGroup {
    type Output = ();
    type IntoFuture = Wait;

    /// Converts the WaitGroup into a future that completes when all tasks finish.
    ///
    /// This allows using `.await` directly on a WaitGroup to wait for all tasks
    /// to complete.
    fn into_future(self) -> Self::IntoFuture {
        let state = self.state.clone();
        drop(self);
        Wait { idx: None, state }
    }
}

/// A future that completes when all tasks in a WaitGroup have finished.
///
/// This type is created by awaiting on a [`WaitGroup`].
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Wait {
    idx: Option<usize>,
    state: Arc<CountdownState>,
}

impl fmt::Debug for Wait {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wait").finish_non_exhaustive()
    }
}

impl Future for Wait {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self { idx, state } = self.get_mut();

        // register waker if the counter is not zero
        if state.spin_wait(16).is_err() {
            state.register_waker(idx, cx);
            // double check after register waker, to catch the update between two steps
            if state.spin_wait(0).is_err() {
                return Poll::Pending;
            }
        }

        Poll::Ready(())
    }
}
