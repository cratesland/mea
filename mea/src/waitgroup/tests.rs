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
use std::time::Duration;

use super::*;
use crate::test_runtime;

#[test]
fn test_wait_group_drop() {
    let wg = WaitGroup::new();
    for _i in 0..100 {
        let w = wg.clone();
        test_runtime().spawn(async move {
            drop(w);
        });
    }
    pollster::block_on(wg.into_future());
}

#[test]
fn test_wait_group_await() {
    let wg = WaitGroup::new();
    for _i in 0..100 {
        let w = wg.clone();
        test_runtime().spawn(async move {
            w.await;
        });
    }
    pollster::block_on(wg.into_future());
}

#[test]
fn test_wait_group_timeout() {
    let wg = WaitGroup::new();
    let _wg_clone = wg.clone();
    let timeout = test_runtime().block_on(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(50)) => true ,
            _ = wg => false,
        }
    });
    assert!(timeout);
}
