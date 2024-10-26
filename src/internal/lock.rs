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

use std::sync::Mutex as StdMutex;
use std::sync::MutexGuard as StdMutexGuard;
use std::sync::PoisonError;

pub(crate) struct Mutex<T>(StdMutex<T>);

impl<T> Mutex<T> {
    #[must_use]
    #[inline]
    pub(crate) const fn new(t: T) -> Self {
        Self(StdMutex::new(t))
    }

    pub(crate) fn lock(&self) -> StdMutexGuard<'_, T> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_poison_mutex() {
        let mutex = Arc::new(Mutex::new(()));

        let m = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = m.lock();
            panic!("poisoned");
        })
        .join();

        let guard = mutex.lock();
        assert_eq!((), *guard);
    }
}
