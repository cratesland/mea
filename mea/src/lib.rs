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

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![deny(missing_docs)]

//! # Mea - Make Easy Async
//!
//! `mea` is a runtime-agnostic library providing essential synchronization primitives for
//! asynchronous Rust programming. The library offers a collection of well-tested, efficient
//! synchronization tools that work with any async runtime.
//!
//! ## Features
//!
//! * [`Barrier`] - A synchronization point where multiple tasks can wait until all participants
//!   arrive
//! * [`Latch`] - A single-use barrier that allows one or more tasks to wait until a signal is given
//! * [`Mutex`] - A mutual exclusion primitive for protecting shared data
//! * [`Semaphore`] - A synchronization primitive that controls access to a shared resource
//! * [`WaitGroup`] - A synchronization primitive that allows waiting for multiple tasks to complete
//!
//! ## Runtime Agnostic
//!
//! All synchronization primitives in this library are runtime-agnostic, meaning they can be used
//! with any async runtime like Tokio, async-std, or others. This makes the library highly versatile
//! and portable.
//!
//! ## Thread Safety
//!
//! All types in this library implement `Send` and `Sync`, making them safe to share across thread
//! boundaries. This is essential for concurrent programming where data needs to be accessed from
//! multiple threads.
//!
//! [`Barrier`]: barrier::Barrier
//! [`Latch`]: latch::Latch
//! [`Mutex`]: mutex::Mutex
//! [`Semaphore`]: semaphore::Semaphore
//! [`WaitGroup`]: waitgroup::WaitGroup

mod internal;

/// A synchronization primitive that enables multiple tasks to wait for each other.
///
/// The barrier ensures that no task proceeds past a certain point until all tasks have reached it.
/// This is useful for scenarios where multiple tasks need to proceed together after reaching a
/// certain point in their execution.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use mea::barrier::Barrier;
///
/// async fn example() {
///     let barrier = Arc::new(Barrier::new(3));
///     let mut handles = Vec::new();
///
///     for i in 0..3 {
///         let barrier = barrier.clone();
///         handles.push(tokio::spawn(async move {
///             println!("Task {} before barrier", i);
///             let is_leader = barrier.wait().await;
///             println!("Task {} after barrier (leader: {})", i, is_leader);
///         }));
///     }
/// }
/// ```
pub mod barrier;

/// A countdown latch that allows one or more tasks to wait until a set of operations completes.
///
/// Unlike a barrier, a latch's count can only decrease and cannot be reused once it reaches zero.
/// This makes it ideal for scenarios where you need to wait for a specific number of events or
/// operations to complete.
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
///     for i in 0..3 {
///         let latch = latch.clone();
///         handles.push(tokio::spawn(async move {
///             println!("Task {} starting", i);
///             // Simulate some work
///             latch.count_down(); // Signal completion
///         }));
///     }
///
///     // Wait for all tasks to complete
///     latch.wait().await;
///     println!("All tasks completed");
/// }
/// ```
pub mod latch;

/// An async mutex for protecting shared data.
///
/// Unlike a standard mutex, this implementation is designed to work with async/await,
/// ensuring tasks yield properly when the lock is contended. This makes it suitable
/// for protecting shared resources in async code.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use mea::mutex::Mutex;
///
/// async fn example() {
///     let mutex = Arc::new(Mutex::new(0));
///     let mut handles = Vec::new();
///
///     for i in 0..3 {
///         let mutex = mutex.clone();
///         handles.push(tokio::spawn(async move {
///             let mut lock = mutex.lock().await;
///             *lock += i;
///         }));
///     }
/// }
/// ```
pub mod mutex;

/// A counting semaphore for controlling access to a pool of resources.
///
/// Semaphores can be used to limit the number of tasks that can access a resource
/// simultaneously. This is particularly useful for implementing connection pools,
/// rate limiters, or any scenario where you need to control concurrent access to
/// limited resources.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use mea::semaphore::Semaphore;
///
/// struct ConnectionPool {
///     sem: Arc<Semaphore>,
/// }
///
/// impl ConnectionPool {
///     fn new(size: u32) -> Self {
///         Self {
///             sem: Arc::new(Semaphore::new(size)),
///         }
///     }
///
///     async fn get_connection(&self) -> Connection {
///         let _permit = self.sem.acquire(1).await;
///         Connection {} // Acquire and return a connection
///     }
/// }
///
/// struct Connection {}
/// ```
pub mod semaphore;

/// A synchronization primitive for waiting on multiple tasks to complete.
///
/// Similar to Go's WaitGroup, this type allows a task to wait for multiple other
/// tasks to finish. Each task holds a handle to the WaitGroup, and the main task
/// can wait for all handles to be dropped before proceeding.
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
///     for i in 0..3 {
///         let wg = wg.clone();
///         handles.push(tokio::spawn(async move {
///             println!("Task {} starting", i);
///             tokio::time::sleep(Duration::from_millis(100)).await;
///             // wg is automatically decremented when dropped
///         }));
///     }
///
///     // Wait for all tasks to complete
///     wg.await;
///     println!("All tasks completed");
/// }
/// ```
pub mod waitgroup;

#[cfg(test)]
fn test_runtime() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;

    use tokio::runtime::Runtime;
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().unwrap())
}

#[cfg(test)]
mod tests {
    use crate::barrier::Barrier;
    use crate::latch::Latch;
    use crate::waitgroup::WaitGroup;

    #[test]
    fn send_and_sync() {
        fn assert_send_and_sync<T: Send + Sync>() {}
        assert_send_and_sync::<Barrier>();
        assert_send_and_sync::<Latch>();
        assert_send_and_sync::<WaitGroup>();
    }
}
