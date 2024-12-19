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
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::primitives::condvar::Condvar;
use crate::primitives::mutex::Mutex;
use crate::test_runtime;

#[test]
fn notify_all() {
    test_runtime().block_on(async {
        let mut tasks: Vec<JoinHandle<()>> = Vec::new();
        let pair = Arc::new((Mutex::new(0u32), Condvar::new()));

        for _ in 0..10 {
            let pair = pair.clone();
            tasks.push(tokio::spawn(async move {
                let (m, c) = &*pair;
                let mut count = m.lock().await;
                while *count == 0 {
                    count = c.wait(count).await;
                }
                *count += 1;
            }));
        }

        // Give some time for tasks to start up
        tokio::time::sleep(Duration::from_millis(50)).await;

        let (m, c) = &*pair;
        {
            let mut count = m.lock().await;
            *count += 1;
            c.notify_all();
        }

        for t in tasks {
            t.await.unwrap();
        }
        let count = m.lock().await;
        assert_eq!(11, *count);
    });
}
