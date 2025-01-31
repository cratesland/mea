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

use std::sync::Arc;
use std::vec::Vec;

use super::*;

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
