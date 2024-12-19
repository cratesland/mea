mod internal;
pub mod barrier;
pub mod condvar;
pub mod latch;
pub mod mutex;
pub mod rwlock;
pub mod semaphore;
pub mod waitgroup;

#[cfg(test)]
mod tests {
    use crate::primitives::barrier::Barrier;
    use crate::primitives::condvar::Condvar;
    use crate::primitives::latch::Latch;
    use crate::primitives::mutex::Mutex;
    use crate::primitives::mutex::MutexGuard;
    use crate::primitives::rwlock::RwLock;
    use crate::primitives::rwlock::RwLockReadGuard;
    use crate::primitives::rwlock::RwLockWriteGuard;
    use crate::primitives::semaphore::Semaphore;
    use crate::primitives::waitgroup::WaitGroup;

    #[test]
    fn assert_send_and_sync() {
        fn do_assert_send_and_sync<T: Send + Sync>() {}
        do_assert_send_and_sync::<Barrier>();
        do_assert_send_and_sync::<Condvar>();
        do_assert_send_and_sync::<Latch>();
        do_assert_send_and_sync::<Semaphore>();
        do_assert_send_and_sync::<WaitGroup>();
        do_assert_send_and_sync::<Mutex<i64>>();
        do_assert_send_and_sync::<MutexGuard<'_, i64>>();
        do_assert_send_and_sync::<RwLock<i64>>();
        do_assert_send_and_sync::<RwLockReadGuard<'_, i64>>();
        do_assert_send_and_sync::<RwLockWriteGuard<'_, i64>>();
    }

    #[test]
    fn assert_unpin() {
        fn do_assert_unpin<T: Unpin>() {}
        do_assert_unpin::<Barrier>();
        do_assert_unpin::<Condvar>();
        do_assert_unpin::<Latch>();
        do_assert_unpin::<Semaphore>();
        do_assert_unpin::<WaitGroup>();
        do_assert_unpin::<Mutex<i64>>();
        do_assert_unpin::<MutexGuard<'_, i64>>();
        do_assert_unpin::<RwLock<i64>>();
        do_assert_unpin::<RwLockReadGuard<'_, i64>>();
        do_assert_unpin::<RwLockWriteGuard<'_, i64>>();
    }
}
