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

use std::time::Duration;

use futures::StreamExt;

use crate::channel::bounded;
use crate::channel::unbounded;
use crate::channel::Sender;
use crate::channel::TryRecvError;
use crate::channel::TrySendError;
use crate::test_runtime;

fn emit_values(tx: Sender<i32>) {
    for i in 0..100 {
        let tx = tx.clone();
        tokio::spawn(async move {
            tx.send(i).await.unwrap();
        });
    }
}

#[test]
fn test_unbounded_sum() {
    let (tx, rx) = unbounded();

    test_runtime().block_on(async move {
        emit_values(tx);

        let sum = rx
            .into_stream()
            .fold(0, |acc, i| async move { acc + i })
            .await;

        assert_eq!(sum, 4950);
    });
}

#[test]
fn test_bounded_sum() {
    let (tx, rx) = bounded(10);

    test_runtime().block_on(async move {
        emit_values(tx);

        tokio::time::sleep(Duration::from_secs(1)).await;

        let sum = rx
            .into_stream()
            .fold(0, |acc, i| async move { acc + i })
            .await;

        assert_eq!(sum, 4950);
    });
}

#[test]
fn test_try_send_recv() {
    let (tx, rx) = bounded(1);

    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

    assert_eq!(tx.try_send(1), Ok(()));
    assert_eq!(tx.try_send(2), Err(TrySendError::Full(2)));

    assert_eq!(rx.try_recv(), Ok(1));
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    drop(rx);

    assert_eq!(tx.try_send(3), Err(TrySendError::Disconnected(3)));
}
