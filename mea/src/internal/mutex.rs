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

#[cfg(feature = "parking_lot")]
pub(crate) use parking_lot::*;

#[cfg(feature = "parking_lot")]
mod parking_lot {
    use std::fmt;
    use std::marker::PhantomData;
    use std::ops::Deref;
    use std::ops::DerefMut;

    pub(crate) struct MutexGuard<'a, T: ?Sized>(
        PhantomData<std::sync::MutexGuard<'a, T>>,
        parking_lot::MutexGuard<'a, T>,
    );

    impl<T: ?Sized> Deref for MutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.1.deref()
        }
    }

    impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            self.1.deref_mut()
        }
    }

    impl<T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.1.fmt(f)
        }
    }

    pub(crate) struct Mutex<T: ?Sized>(PhantomData<std::sync::Mutex<T>>, parking_lot::Mutex<T>);

    impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.1.fmt(f)
        }
    }

    impl<T> Mutex<T> {
        pub(crate) const fn new(t: T) -> Self {
            Mutex(PhantomData, parking_lot::Mutex::new(t))
        }

        pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
            MutexGuard(PhantomData, self.1.lock())
        }
    }
}

#[cfg(not(feature = "parking_lot"))]
pub(crate) use std_lock::*;

#[cfg(not(feature = "parking_lot"))]
mod std_lock {
    use std::fmt;
    use std::sync::PoisonError;

    pub(crate) type MutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;

    pub(crate) struct Mutex<T: ?Sized>(std::sync::Mutex<T>);

    impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }

    impl<T> Mutex<T> {
        pub(crate) const fn new(t: T) -> Self {
            Mutex(std::sync::Mutex::new(t))
        }

        pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
            self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn test_poison_mutex() {
        let mutex = Arc::new(Mutex::new(42));
        let m = mutex.clone();
        let handle = std::thread::spawn(move || {
            let _guard = m.lock();
            panic!("poison");
        });
        let _ = handle.join();
        let guard = mutex.lock();
        assert_eq!(*guard, 42);
    }
}
