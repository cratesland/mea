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
//! * [`Barrier`]: A synchronization point where multiple tasks can wait until all participants
//!   arrive
//! * [`Condvar`]: A condition variable that allows tasks to wait for a notification
//! * [`Latch`]: A single-use barrier that allows one or more tasks to wait until a signal is given
//! * [`Mutex`]: A mutual exclusion primitive for protecting shared data
//! * [`RwLock`]: A reader-writer lock that allows multiple readers or a single writer at a time
//! * [`Semaphore`]: A synchronization primitive that controls access to a shared resource
//! * [`WaitGroup`]: A synchronization primitive that allows waiting for multiple tasks to complete
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
//! [`Condvar`]: condvar::Condvar
//! [`Latch`]: latch::Latch
//! [`Mutex`]: mutex::Mutex
//! [`Semaphore`]: semaphore::Semaphore
//! [`WaitGroup`]: waitgroup::WaitGroup

mod internal;

pub mod barrier;
pub mod condvar;
pub mod latch;
pub mod mutex;
pub mod rwlock;
pub mod semaphore;
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
    use crate::condvar::Condvar;
    use crate::latch::Latch;
    use crate::mutex::Mutex;
    use crate::mutex::MutexGuard;
    use crate::rwlock::RwLock;
    use crate::rwlock::RwLockReadGuard;
    use crate::rwlock::RwLockWriteGuard;
    use crate::semaphore::Semaphore;
    use crate::waitgroup::WaitGroup;

    #[test]
    fn send_and_sync() {
        fn assert_send_and_sync<T: Send + Sync>() {}
        assert_send_and_sync::<Barrier>();
        assert_send_and_sync::<Condvar>();
        assert_send_and_sync::<Latch>();
        assert_send_and_sync::<Semaphore>();
        assert_send_and_sync::<WaitGroup>();

        assert_send_and_sync::<Mutex<i64>>();
        assert_send_and_sync::<MutexGuard<'_, i64>>();
        assert_send_and_sync::<RwLock<i64>>();
        assert_send_and_sync::<RwLockReadGuard<'_, i64>>();
        assert_send_and_sync::<RwLockWriteGuard<'_, i64>>();
    }
}
