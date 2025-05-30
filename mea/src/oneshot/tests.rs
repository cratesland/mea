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

use std::future::IntoFuture;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::oneshot;

struct DropCounterHandle(Arc<AtomicUsize>);

impl DropCounterHandle {
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

struct DropCounter<T> {
    drop_count: Arc<AtomicUsize>,
    value: Option<T>,
}

impl<T> DropCounter<T> {
    fn new(value: T) -> (Self, DropCounterHandle) {
        let drop_count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                drop_count: drop_count.clone(),
                value: Some(value),
            },
            DropCounterHandle(drop_count),
        )
    }

    fn value(&self) -> &T {
        self.value.as_ref().unwrap()
    }
}

impl<T> Drop for DropCounter<T> {
    fn drop(&mut self) {
        self.drop_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn send_before_await() {
    let (sender, receiver) = oneshot::channel();
    assert!(sender.send(19i128).is_ok());
    assert_eq!(receiver.await, Ok(19i128));
}

#[tokio::test]
async fn await_with_dropped_sender() {
    let (sender, receiver) = oneshot::channel::<u128>();
    drop(sender);
    receiver.await.unwrap_err();
}

#[tokio::test]
async fn await_before_send() {
    let (sender, receiver) = oneshot::channel();
    let (message, counter) = DropCounter::new(79u128);
    let t = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        sender.send(message)
    });
    let returned_message = receiver.await.unwrap();
    assert_eq!(counter.count(), 0);
    assert_eq!(*returned_message.value(), 79u128);
    drop(returned_message);
    assert_eq!(counter.count(), 1);
    t.await.unwrap().unwrap();
}

#[tokio::test]
async fn await_before_send_then_drop_sender() {
    let (sender, receiver) = oneshot::channel::<u128>();
    let t = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(sender);
    });
    assert!(receiver.await.is_err());
    t.await.unwrap();
}

#[tokio::test]
async fn poll_receiver_then_drop_it() {
    let (sender, receiver) = oneshot::channel::<()>();
    // This will poll the receiver and then give up after 100 ms.
    tokio::time::timeout(Duration::from_millis(100), receiver)
        .await
        .unwrap_err();
    // Make sure the receiver has been dropped by the runtime.
    assert!(sender.send(()).is_err());
}

#[tokio::test]
async fn recv_within_select() {
    let (tx, rx) = oneshot::channel::<&'static str>();
    let mut interval = tokio::time::interval(Duration::from_secs(100));

    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send("shut down").unwrap();
    });

    let mut recv = rx.into_future();
    loop {
        tokio::select! {
            _ = interval.tick() => println!("another 100ms"),
            msg = &mut recv => {
                println!("Got message: {}", msg.unwrap());
                break;
            }
        }
    }

    handle.await.unwrap();
}
