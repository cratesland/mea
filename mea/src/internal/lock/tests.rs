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

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::vec::Vec;

use super::*;

#[test]
fn test_lock() {
    let mutex = Mutex::new(42);
    assert_eq!(42, mutex.with(|v| *v));
}

struct UnsafeSendCell<T>(UnsafeCell<T>);
unsafe impl<T> Send for UnsafeSendCell<T> {}
unsafe impl<T> Sync for UnsafeSendCell<T> {}
impl<T> UnsafeSendCell<T> {
    fn new(t: T) -> Self {
        Self(UnsafeCell::new(t))
    }
    unsafe fn get(&self) -> *mut T {
        self.0.get()
    }
}

#[test]
fn test_multi_thread() {
    let mutex = Arc::new(Mutex::new(()));
    let counter = Arc::new(UnsafeSendCell::new(0_usize));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let mutex = mutex.clone();
            let counter = counter.clone();
            std::thread::spawn(move || {
                mutex.with(|_| unsafe { *counter.get() += 1 });
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    mutex.with(|_| assert_eq!(unsafe { *counter.get() }, 4));
}
