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

use std::sync::PoisonError;

pub(crate) struct Mutex<T>(std::sync::Mutex<T>);

impl<T> Mutex<T> {
    pub(crate) const fn new(t: T) -> Self {
        Self(std::sync::Mutex::new(t))
    }

    pub(crate) fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let mut this = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut this)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn test_poison_mutex() {
        let m = Arc::new(Mutex::new(42));

        let m_clone = m.clone();
        let _ = std::thread::spawn(move || {
            m_clone.with(|_| panic!("poisoned"));
        })
        .join();

        assert_eq!(42, m.with(|v| *v));
    }
}
