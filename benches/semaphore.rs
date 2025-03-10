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

use divan::Bencher;
use mea::semaphore::Semaphore;

fn multi_rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(6)
        .build()
        .unwrap()
}

async fn task(s: Arc<Semaphore>) {
    let permit = s.acquire(1).await;
    drop(permit);
}

#[divan::bench]
fn benchmark_contention(bencher: Bencher) {
    bencher.bench(|| {
        let rt = multi_rt();
        let s = Arc::new(Semaphore::new(5));
        rt.block_on(async move {
            let j = tokio::try_join! {
                tokio::task::spawn(task(s.clone())),
                tokio::task::spawn(task(s.clone())),
                tokio::task::spawn(task(s.clone())),
                tokio::task::spawn(task(s.clone())),
                tokio::task::spawn(task(s.clone())),
                tokio::task::spawn(task(s.clone()))
            };
            j.unwrap();
        })
    });
}

fn main() {
    divan::main()
}
