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

use std::time::Instant;

use tokio_test::assert_ok;

use crate::mpsc;
use crate::mpsc::TryRecvError;
use crate::mpsc::TrySendError;
use crate::test_runtime;

#[test]
fn test_unbounded_pressure() {
    let n = 1024 * 1024;
    let (tx, mut rx) = mpsc::unbounded();

    test_runtime().block_on(async move {
        let start = Instant::now();
        tokio::spawn(async move {
            for i in 0..n {
                tx.send(i).unwrap();
            }
        });

        for i in 0..n {
            assert_eq!(rx.recv().await, Some(i));
        }
        println!("Elapsed: {:?}", start.elapsed());
    });
}

#[test]
fn test_unbounded_sum() {
    let (tx, mut rx) = mpsc::unbounded();

    test_runtime().block_on(async move {
        for i in 0..100 {
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(i).unwrap();
            });
        }
        drop(tx);

        let mut sum = 0;
        while let Some(i) = rx.recv().await {
            sum += i;
        }
        assert_eq!(sum, 4950);
    });
}

#[tokio::test]
async fn select_streams() {
    let (tx1, mut rx1) = mpsc::unbounded::<i32>();
    let (tx2, mut rx2) = mpsc::unbounded::<i32>();
    let (tx3, mut rx3) = mpsc::bounded(1);
    let (tx4, mut rx4) = mpsc::bounded(1);

    tokio::spawn(async move {
        assert_ok!(tx2.send(1));
        tokio::task::yield_now().await;

        assert_ok!(tx1.send(2));
        tokio::task::yield_now().await;

        assert_ok!(tx2.send(3));
        tokio::task::yield_now().await;

        assert_ok!(tx3.send(4).await);
        tokio::task::yield_now().await;

        assert_ok!(tx4.send(5).await);
        tokio::task::yield_now().await;

        assert_ok!(tx3.send(6).await);
        tokio::task::yield_now().await;

        drop((tx1, tx2));
    });

    let mut rem = true;
    let mut msgs = vec![];

    while rem {
        tokio::select! {
            Some(x) = rx1.recv() => {
                msgs.push(x);
            }
            Some(y) = rx2.recv() => {
                msgs.push(y);
            }
            Some(z) = rx3.recv() => {
                msgs.push(z);
            }
            Some(w) = rx4.recv() => {
                msgs.push(w);
            }
            else => {
                rem = false;
            }
        }
    }

    msgs.sort_unstable();
    assert_eq!(&msgs[..], &[1, 2, 3, 4, 5, 6]);
}

#[tokio::test]
async fn send_recv_unbounded() {
    let (tx, mut rx) = mpsc::unbounded::<i32>();

    // Using `try_send`
    assert_ok!(tx.send(1));
    assert_ok!(tx.send(2));

    assert_eq!(rx.recv().await, Some(1));
    assert_eq!(rx.recv().await, Some(2));

    drop(tx);

    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn async_send_recv_unbounded() {
    let (tx, mut rx) = mpsc::unbounded();

    tokio::spawn(async move {
        assert_ok!(tx.send(1));
        assert_ok!(tx.send(2));
    });

    assert_eq!(Some(1), rx.recv().await);
    assert_eq!(Some(2), rx.recv().await);
    assert_eq!(None, rx.recv().await);
}

#[test]
fn try_recv_unbounded() {
    for num in 0..100 {
        let (tx, mut rx) = mpsc::unbounded();

        for i in 0..num {
            tx.send(i).unwrap();
        }

        for i in 0..num {
            assert_eq!(rx.try_recv(), Ok(i));
        }

        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        drop(tx);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
}

#[test]
fn try_recv_close_while_empty_unbounded() {
    let (tx, mut rx) = mpsc::unbounded::<()>();

    assert_eq!(Err(TryRecvError::Empty), rx.try_recv());
    drop(tx);
    assert_eq!(Err(TryRecvError::Disconnected), rx.try_recv());
}

#[tokio::test]
async fn send_recv_bounded() {
    let (tx, mut rx) = mpsc::bounded(1);

    tx.send(1).await.unwrap();
    assert_eq!(rx.recv().await, Some(1));

    drop(tx);
    assert_eq!(rx.recv().await, None);
}

#[tokio::test]
async fn async_send_recv_bounded() {
    let (tx, mut rx) = mpsc::bounded(1);

    tx.send(1).await.unwrap();
    // This will block until the receiver is ready to receive.
    tokio::spawn(async move {
        tx.send(2).await.unwrap();
    });

    assert_eq!(Some(1), rx.recv().await);
    assert_eq!(Some(2), rx.recv().await);
    assert_eq!(None, rx.recv().await);
}

#[test]
fn try_send_recv_bounded() {
    for num in 1..101 {
        let (tx, mut rx) = mpsc::bounded(num);

        for i in 0..num {
            tx.try_send(i).unwrap();
        }

        assert_eq!(tx.try_send(num), Err(TrySendError::Full(num)));

        for i in 0..num {
            assert_eq!(rx.try_recv(), Ok(i));
        }

        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        drop(tx);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
}

#[tokio::test]
async fn try_send_after_close_bounded() {
    let (tx, rx) = mpsc::bounded(1);

    tx.try_send(1).unwrap();
    drop(rx);

    assert_eq!(tx.try_send(3), Err(TrySendError::Disconnected(3)));
}

#[tokio::test]
async fn send_after_close_bounded() {
    let (tx, mut rx) = mpsc::bounded(1);

    tx.send(1).await.unwrap();
    assert_eq!(rx.recv().await, Some(1));

    drop(rx);
    assert_eq!(tx.send(2).await, Err(mpsc::SendError::new(2)));
}

#[test]
fn test_bounded_pressure() {
    let n = 1024 * 1024;
    let (tx, mut rx) = mpsc::bounded(1024);

    test_runtime().block_on(async move {
        let start = Instant::now();
        tokio::spawn(async move {
            for i in 0..n {
                tx.send(i).await.unwrap();
            }
        });

        for i in 0..n {
            assert_eq!(rx.recv().await, Some(i));
        }
        println!("Elapsed: {:?}", start.elapsed());
    });
}
