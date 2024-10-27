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

use super::*;
use std::sync::Arc;

#[test]
fn no_permits() {
    // this should not panic
    Semaphore::new(0);
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
