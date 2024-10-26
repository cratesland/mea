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

use core::cell::RefCell;

pub(crate) struct Mutex<T>(critical_section::Mutex<RefCell<T>>);

impl<T> Mutex<T> {
    pub(crate) const fn new(t: T) -> Self {
        Self(critical_section::Mutex::new(RefCell::new(t)))
    }

    pub(crate) fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        critical_section::with(|cs| {
            let mut this = self
                .0
                .borrow(cs)
                .try_borrow_mut()
                // SAFETY: The outer with closure guarantees to borrow with token 'cs' exactly once.
                .expect("[BUG] borrow the mutex multiple times");
            f(&mut this)
        })
    }
}
