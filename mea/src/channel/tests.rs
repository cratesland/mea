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

use futures::StreamExt;

use crate::channel::unbounded;
use crate::test_runtime;

#[test]
fn test_unbounded() {
    let (tx, rx) = unbounded();

    test_runtime().block_on(async move {
        for i in 0..100 {
            let tx = tx.clone();
            tokio::spawn(async move {
                tx.send(i).await.unwrap();
            });
        }
        drop(tx);

        let sum = rx
            .into_stream()
            .fold(0, |acc, i| async move { acc + i })
            .await;

        // let mut sum = 0;
        // while let Ok(i) = rx.recv().await {
        //     sum += i;
        // }
        assert_eq!(sum, 4950);
    });
}
