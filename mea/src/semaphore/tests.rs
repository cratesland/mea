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

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::Context;
use std::task::RawWaker;
use std::task::RawWakerVTable;
use std::task::Waker;
use std::vec::Vec;

use super::*;
use crate::latch::Latch;

#[test]
fn no_permits() {
    // this should not panic
    Semaphore::new(0);
}

#[test]
fn try_acquire() {
    let sem = Semaphore::new(1);
    {
        let p1 = sem.try_acquire(1);
        assert!(p1.is_some());
        let p2 = sem.try_acquire(1);
        assert!(p2.is_none());
    }
    let p3 = sem.try_acquire(1);
    assert!(p3.is_some());
}

#[tokio::test]
async fn acquire() {
    let sem = Arc::new(Semaphore::new(1));
    let p1 = sem.try_acquire(1).unwrap();
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move {
        let _p2 = sem_clone.acquire(1).await;
    });
    drop(p1);
    j.await.unwrap();
}

#[tokio::test]
async fn add_permits() {
    let sem = Arc::new(Semaphore::new(0));
    let sem_clone = sem.clone();
    let j = tokio::spawn(async move {
        let _p2 = sem_clone.acquire(1).await;
    });
    sem.release(1);
    j.await.unwrap();
}

#[test]
fn forget() {
    let sem = Arc::new(Semaphore::new(1));
    {
        let p = sem.try_acquire(1).unwrap();
        assert_eq!(sem.available_permits(), 0);
        p.forget();
        assert_eq!(sem.available_permits(), 0);
    }
    assert_eq!(sem.available_permits(), 0);
    assert!(sem.try_acquire(1).is_none());
}

#[tokio::test]
async fn stress_test() {
    let sem = Arc::new(Semaphore::new(5));
    let mut join_handles = Vec::new();
    for i in 0..100 {
        let sem_clone = sem.clone();
        join_handles.push(tokio::spawn(async move {
            let _p = sem_clone.acquire(1).await;
            tokio::time::sleep(std::time::Duration::from_millis(100 - i)).await;
        }));
    }
    for j in join_handles {
        j.await.unwrap();
    }
    // there should be exactly 5 semaphores available now
    let _p1 = sem.try_acquire(1).unwrap();
    let _p2 = sem.try_acquire(1).unwrap();
    let _p3 = sem.try_acquire(1).unwrap();
    let _p4 = sem.try_acquire(1).unwrap();
    let _p5 = sem.try_acquire(1).unwrap();
    assert!(sem.try_acquire(1).is_none());
}

#[test]
fn add_max_amount_permits() {
    let s = Semaphore::new(0);
    s.release(usize::MAX);
    assert_eq!(s.available_permits(), usize::MAX);
}

#[test]
#[should_panic]
fn add_more_than_max_amount_permits1() {
    let s = Semaphore::new(1);
    s.release(usize::MAX);
}

#[test]
#[should_panic]
fn add_more_than_max_amount_permits2() {
    let s = Semaphore::new(usize::MAX - 1);
    s.release(1);
    s.release(1);
}

#[test]
fn no_panic_at_max_permits() {
    let _ = Semaphore::new(usize::MAX);
    let s = Semaphore::new(usize::MAX - 1);
    s.release(1);
}

#[test]
fn try_acquire_concurrently() {
    let s = Semaphore::new(1);
    let p1 = s.try_acquire(1).unwrap();
    assert_eq!(s.available_permits(), 0);
    let p2 = s.try_acquire(1);
    assert!(p2.is_none());
    assert_eq!(s.available_permits(), 0);
    drop(p1);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn acquire_then_drop() {
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);

    let s = Semaphore::new(1);
    let p1 = s.try_acquire(1).unwrap();
    {
        let p2 = s.acquire(1);
        let poll = pin!(p2).poll(&mut context);
        assert!(poll.is_pending());
    }
    drop(p1);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn wake_then_drop() {
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);

    let s = Semaphore::new(2);
    let p1 = s.try_acquire(2).unwrap();
    {
        let p2 = s.acquire(1);
        let p2 = pin!(p2);
        assert!(p2.poll(&mut context).is_pending());
        {
            let p3 = s.acquire(1);
            let p3 = pin!(p3);
            assert!(p3.poll(&mut context).is_pending());
            drop(p1);
        }
    }
    assert_eq!(s.available_permits(), 2);
}

#[tokio::test]
async fn acquire_then_forget_exact() {
    let s = Arc::new(Semaphore::new(5));
    s.forget_exact(3);
    assert_eq!(s.available_permits(), 2);

    let acquired = Arc::new(Latch::new(1));

    let acquired_clone = acquired.clone();
    let s_clone = s.clone();
    tokio::spawn(async move {
        let _p = s_clone.acquire(3).await;
        acquired_clone.count_down();
    });
    assert!(acquired.try_wait().is_err());

    s.forget_exact(2);
    s.release(2);
    assert!(acquired.try_wait().is_err());

    s.release(1);
    acquired.wait().await;
    assert_eq!(s.available_permits(), 3);
}

fn noop_waker() -> Waker {
    const NOOP: RawWaker = {
        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            // Cloning just returns a new no-op raw waker
            |_| NOOP,
            // `wake` does nothing
            |_| {},
            // `wake_by_ref` does nothing
            |_| {},
            // Dropping does nothing as we don't allocate anything
            |_| {},
        );
        RawWaker::new(std::ptr::null(), &VTABLE)
    };

    unsafe { Waker::from_raw(NOOP) }
}
