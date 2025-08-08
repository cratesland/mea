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
use std::sync::Weak;

use super::*;
use crate::rwlock::OwnedMappedRwLockReadGuard;
use crate::rwlock::OwnedMappedRwLockWriteGuard;
use crate::rwlock::OwnedRwLockReadGuard;
use crate::rwlock::OwnedRwLockWriteGuard;

#[test]
fn test_try_read_write_never_blocks() {
    // Test that try_read and try_write never block
    let rwlock = Arc::new(RwLock::new(42));

    let r1 = rwlock.try_read().unwrap();
    let _r2 = rwlock.try_read().unwrap();
    assert_eq!(*r1, 42);
    assert_eq!(*_r2, 42);

    assert!(rwlock.try_write().is_none());

    drop(r1);
    drop(_r2);

    let w = rwlock.try_write().unwrap();
    assert_eq!(*w, 42);

    assert!(rwlock.try_read().is_none());
    assert!(rwlock.clone().try_read_owned().is_none());
}

#[test]
fn test_get_mut_provides_exclusive_access() {
    // Test that get_mut provides exclusive access to the data
    let mut rwlock = RwLock::new(100);

    let data = rwlock.get_mut();
    *data = 200;

    assert_eq!(*rwlock.get_mut(), 200);

    let inner = rwlock.into_inner();
    assert_eq!(inner, 200);
}

#[test]
fn test_with_max_readers() {
    let rwlock = RwLock::with_max_readers(10, 2);

    let r1 = rwlock.try_read().unwrap();
    let r2 = rwlock.try_read().unwrap();
    assert_eq!(*r1, 10);
    assert_eq!(*r2, 10);

    assert!(rwlock.try_read().is_none());

    assert!(rwlock.try_write().is_none());

    drop(r1);

    let r3 = rwlock.try_read().unwrap();
    assert_eq!(*r3, 10);

    assert!(rwlock.try_read().is_none());

    drop(r2);
    drop(r3);

    let mut w = rwlock.try_write().unwrap();
    *w = 20;
    drop(w);

    let r = rwlock.try_read().unwrap();
    assert_eq!(*r, 20);
}

#[tokio::test]
async fn test_stress_concurrent_readers_writers() {
    // Test concurrent readers and writers with RwLock
    let rwlock = Arc::new(RwLock::new(0i32));
    let mut reader_results = Vec::new();
    let mut writer_results = Vec::new();

    // Spawn reader tasks
    for i in 0..50 {
        let rwlock_clone = rwlock.clone();
        let handle = tokio::spawn(async move {
            let guard = rwlock_clone.read().await;
            let value = *guard;
            tokio::task::yield_now().await;
            (i, value)
        });
        reader_results.push(handle);
    }

    // Spawn writer tasks
    for i in 0..10 {
        let rwlock_clone = rwlock.clone();
        let handle = tokio::spawn(async move {
            let mut guard = rwlock_clone.write().await;
            let old_value = *guard;
            *guard += 1;
            tokio::task::yield_now().await;
            (i + 100, old_value, *guard)
        });
        writer_results.push(handle);
    }

    for handle in reader_results {
        let (reader_id, value) = handle.await.unwrap();
        assert!(
            (0..=10).contains(&value),
            "Reader {} saw invalid value: {}",
            reader_id,
            value
        );
    }

    let mut writer_values = Vec::new();
    for handle in writer_results {
        let (writer_id, old_value, new_value) = handle.await.unwrap();
        assert_eq!(
            new_value,
            old_value + 1,
            "Writer {} increment failed: {} -> {}",
            writer_id,
            old_value,
            new_value
        );
        writer_values.push((old_value, new_value));
    }

    let final_guard = rwlock.read().await;
    assert_eq!(
        *final_guard, 10,
        "Final value should be 10 after 10 increments"
    );

    writer_values.sort_by_key(|(old, _)| *old);
    for (i, (old_value, new_value)) in writer_values.iter().enumerate() {
        assert_eq!(
            *old_value, i as i32,
            "Writer operations should be sequential"
        );
        assert_eq!(
            *new_value,
            (i + 1) as i32,
            "Each increment should be atomic"
        );
    }
}

#[tokio::test]
async fn test_guard_prevents_concurrent_access() {
    let rwlock = Arc::new(RwLock::new(0));
    let rwlock_clone = rwlock.clone();
    let writer_queued = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer_queued_clone = writer_queued.clone();
    let writer_completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer_completed_clone = writer_completed.clone();

    let read_guard = rwlock.read().await;

    assert!(rwlock.try_write().is_none());

    let handle = tokio::spawn(async move {
        writer_queued_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut write_guard = rwlock_clone.write().await;
        *write_guard = 123;
        writer_completed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        *write_guard
    });

    while !writer_queued.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }

    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    assert!(!writer_completed.load(std::sync::atomic::Ordering::SeqCst));
    assert!(rwlock.try_write().is_none());

    drop(read_guard);

    let result = handle.await.unwrap();
    assert_eq!(result, 123);
    assert!(writer_completed.load(std::sync::atomic::Ordering::SeqCst));

    // Verify the write took effect and lock is available
    let final_guard = rwlock.try_read().unwrap();
    assert_eq!(*final_guard, 123);
}

#[test]
fn test_lock_panic_safety() {
    // Test panic safety with synchronous locks
    use std::panic::AssertUnwindSafe;

    let rwlock = Arc::new(RwLock::new(0));
    let rwlock_clone = rwlock.clone();

    let result = std::panic::catch_unwind(AssertUnwindSafe(move || {
        let _guard = rwlock_clone.try_read().unwrap();
        panic!("test panic");
    }));

    assert!(result.is_err());
    // Lock should be released after panic
    assert!(rwlock.try_read().is_some());
}

#[tokio::test]
async fn test_async_lock_panic_safety() {
    // Test panic safety with async locks
    let rwlock = Arc::new(RwLock::new(0));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let _guard = rwlock_clone.read().await;
        panic!("async test panic");
    });

    // panic
    assert!(handle.await.is_err());

    let guard = rwlock.try_read();
    assert!(guard.is_some());
}

#[tokio::test]
async fn test_owned_guard_panic_safety() {
    // Test panic safety with owned guards
    let rwlock = Arc::new(RwLock::new(0));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let _guard = rwlock_clone.clone().read_owned().await;
        panic!("owned guard panic");
    });

    assert!(handle.await.is_err());

    // Lock should be available after the panicked task
    let guard = rwlock.try_read();
    assert!(guard.is_some());
}

#[tokio::test]
async fn test_mapped_guard_panic_safety() {
    // Test panic safety with mapped guards
    let rwlock = Arc::new(RwLock::new((66, vec![1, 2, 3])));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let guard = rwlock_clone.read().await;
        let _mapped = RwLockReadGuard::map(guard, |data| &data.0);
        panic!("mapped guard panic");
    });

    assert!(handle.await.is_err());

    let guard = rwlock.try_read();
    assert!(guard.is_some());
}

#[tokio::test]
async fn test_mapped_write_guard_panic_safety() {
    // Test panic safety with mapped write guards
    let rwlock = Arc::new(RwLock::new((42, String::from("test"))));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let guard = rwlock_clone.write().await;
        let _mapped = RwLockWriteGuard::map(guard, |data| &mut data.0);
        panic!("mapped write guard panic");
    });

    assert!(handle.await.is_err());

    let guard = rwlock.try_write();
    assert!(guard.is_some());
}

#[tokio::test]
async fn test_owned_mapped_read_guard_panic_safety() {
    // Test panic safety with owned mapped read guards
    let rwlock = Arc::new(RwLock::new((100, vec![4, 5, 6])));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let guard = rwlock_clone.read_owned().await;
        let _mapped = OwnedRwLockReadGuard::map(guard, |data| &data.1);
        panic!("owned mapped read guard panic");
    });

    assert!(handle.await.is_err());

    let guard = rwlock.try_read();
    assert!(guard.is_some());
}

#[tokio::test]
async fn test_owned_mapped_write_guard_panic_safety() {
    // Test panic safety with owned mapped write guards
    let rwlock = Arc::new(RwLock::new((200, String::from("owned"))));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let guard = rwlock_clone.write_owned().await;
        let _mapped = OwnedRwLockWriteGuard::map(guard, |data| &mut data.1);
        panic!("owned mapped write guard panic");
    });

    assert!(handle.await.is_err());

    let guard = rwlock.try_write();
    assert!(guard.is_some());
}

#[tokio::test]
async fn test_memory_ordering_correctness() {
    // Test that rwlock provides proper memory ordering guarantees
    // When one task modifies data under rwlock protection,
    // another task should see the modification after acquiring the lock
    let rwlock = Arc::new(RwLock::new(vec![1, 2, 3]));
    let rwlock_clone = rwlock.clone();

    let handle = tokio::spawn(async move {
        let mut guard = rwlock_clone.write().await;
        guard.push(4);
        guard[0] = 100;
        // Lock is released when guard is dropped
    });

    handle.await.unwrap();

    let guard = rwlock.read().await;
    assert_eq!(*guard, vec![100, 2, 3, 4]);
}

#[tokio::test]
async fn test_rwlock_debug_when_locked() {
    let rwlock = Arc::new(RwLock::new(78));

    let rwlock_debug_unlocked = format!("{rwlock:?}");
    assert!(
        rwlock_debug_unlocked.contains("78"),
        "RwLock Debug should show value when unlocked, got: {rwlock_debug_unlocked}"
    );

    let write_guard = rwlock.write().await;
    let rwlock_debug_write = format!("{rwlock:?}");
    assert!(
        rwlock_debug_write.contains("<locked>"),
        "RwLock Debug should show <locked> when write lock is held, got: {rwlock_debug_write}"
    );
    drop(write_guard);

    let read_guard = rwlock.read().await;
    let rwlock_debug_read = format!("{rwlock:?}");

    let shows_value = rwlock_debug_read.contains("78");
    let shows_locked = rwlock_debug_read.contains("<locked>");
    assert!(shows_value || shows_locked,
            "RwLock Debug with read lock should show either value or <locked>, got: {rwlock_debug_read}");

    drop(read_guard);

    let rwlock_debug_final = format!("{rwlock:?}");
    assert!(
        rwlock_debug_final.contains("78"),
        "RwLock Debug should show value when all locks are released, got: {rwlock_debug_final}"
    );
}

#[tokio::test]
async fn test_rwlock_zst() {
    // Test that RwLock works correctly with Zero-Sized Types
    let rwlock = Arc::new(RwLock::new(()));

    let rwlock_clone = rwlock.clone();
    let handle = tokio::spawn(async move {
        let guard = rwlock_clone.read().await;
        *guard;
    });

    handle.await.unwrap();

    let guard1 = rwlock.read().await;
    let guard2 = rwlock.clone().read_owned().await;
    *guard1;
    *guard2;

    assert!(rwlock.try_write().is_none());

    drop(guard1);
    drop(guard2);

    let mut write_guard = rwlock.write().await;
    *write_guard = ();
    drop(write_guard);

    let try_write_guard = rwlock.try_write().unwrap();
    *try_write_guard;
    drop(try_write_guard);

    let guard = rwlock.try_read().unwrap();
    *guard;
}

#[tokio::test]
async fn test_owned_write_guard_map_memory_leak() {
    // Test OwnedRwLockWriteGuard::map memory management
    let rwlock = Arc::new(RwLock::new(29u32));
    let weak_ref: Weak<RwLock<u32>> = Arc::downgrade(&rwlock);

    {
        let write_guard = rwlock.clone().write_owned().await;
        let mut mapped_guard = OwnedRwLockWriteGuard::map(write_guard, |data| data);
        *mapped_guard = 100;
        assert_eq!(*mapped_guard, 100);
    }

    drop(rwlock);
    assert!(
        weak_ref.upgrade().is_none(),
        "expected Arc to be dropped (no strong refs)"
    );
}

#[tokio::test]
async fn test_owned_write_guard_filter_map_memory_leak() {
    // Test OwnedRwLockWriteGuard::filter_map memory management

    // Test success case
    {
        let rwlock = Arc::new(RwLock::new(Some(29u32)));
        let weak_ref: Weak<RwLock<Option<u32>>> = Arc::downgrade(&rwlock);

        {
            let write_guard = rwlock.clone().write_owned().await;
            let mut mapped_guard =
                OwnedRwLockWriteGuard::filter_map(write_guard, |data| data.as_mut())
                    .expect("Should succeed");
            *mapped_guard = 100;
            assert_eq!(*mapped_guard, 100);
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "expected Arc to be dropped after filter_map success"
        );
    }

    // Test failure case
    {
        let rwlock = Arc::new(RwLock::new(None::<u32>));
        let weak_ref: Weak<RwLock<Option<u32>>> = Arc::downgrade(&rwlock);

        {
            let write_guard = rwlock.clone().write_owned().await;
            let result = OwnedRwLockWriteGuard::filter_map(write_guard, |data| data.as_mut());
            assert!(result.is_err(), "filter_map should have failed for None");
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "expected Arc to be dropped after filter_map failure"
        );
    }
}

#[tokio::test]
async fn test_owned_mapped_write_guard_map_memory_leak() {
    // Test OwnedMappedRwLockWriteGuard::map memory management
    let rwlock = Arc::new(RwLock::new("test".to_string()));
    let weak_ref: Weak<RwLock<String>> = Arc::downgrade(&rwlock);

    {
        let write_guard = rwlock.clone().write_owned().await;
        let mapped_guard1 = OwnedRwLockWriteGuard::map(write_guard, |s| s);
        let mut mapped_guard2 = OwnedMappedRwLockWriteGuard::map(mapped_guard1, |s| s.as_mut_str());
        mapped_guard2.make_ascii_uppercase();
        assert_eq!(&*mapped_guard2, "TEST");
    }

    drop(rwlock);
    assert!(
        weak_ref.upgrade().is_none(),
        "expected Arc to be dropped (no strong refs)"
    );
}

#[tokio::test]
async fn test_owned_mapped_write_guard_filter_map_memory_leak() {
    // Test OwnedMappedRwLockWriteGuard::filter_map memory management

    // Test success case
    {
        let rwlock = Arc::new(RwLock::new(vec![1, 2, 3]));
        let weak_ref: Weak<RwLock<Vec<i32>>> = Arc::downgrade(&rwlock);

        {
            let write_guard = rwlock.clone().write_owned().await;
            let mapped_guard1 = OwnedRwLockWriteGuard::map(write_guard, |v| v);
            let mut mapped_guard2 = OwnedMappedRwLockWriteGuard::filter_map(mapped_guard1, |v| {
                if !v.is_empty() {
                    Some(&mut v[0])
                } else {
                    None
                }
            })
            .expect("Should succeed");
            *mapped_guard2 = 100;
            assert_eq!(*mapped_guard2, 100);
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "Memory leak detected on filter_map success"
        );
    }

    // Test failure case
    {
        let rwlock = Arc::new(RwLock::new(Vec::<i32>::new()));
        let weak_ref: Weak<RwLock<Vec<i32>>> = Arc::downgrade(&rwlock);

        {
            let write_guard = rwlock.clone().write_owned().await;
            let mapped_guard1 = OwnedRwLockWriteGuard::map(write_guard, |v| v);
            let result = OwnedMappedRwLockWriteGuard::filter_map(mapped_guard1, |v| {
                if !v.is_empty() {
                    Some(&mut v[0])
                } else {
                    None
                }
            });

            assert!(
                result.is_err(),
                "filter_map should have failed for empty vector"
            );
            if let Err(original_guard) = result {
                assert!(original_guard.is_empty());
            }
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "Memory leak detected on filter_map failure"
        );
    }
}

#[tokio::test]
async fn test_owned_read_guard_map_memory_leak() {
    // Test OwnedRwLockReadGuard::map memory management
    let rwlock = Arc::new(RwLock::new(29u32));
    let weak_ref: Weak<RwLock<u32>> = Arc::downgrade(&rwlock);

    {
        let read_guard = rwlock.clone().read_owned().await;
        let mapped_guard = OwnedRwLockReadGuard::map(read_guard, |data| data);
        assert_eq!(*mapped_guard, 29);
    }

    drop(rwlock);
    assert!(
        weak_ref.upgrade().is_none(),
        "expected Arc to be dropped (no strong ref; potential memory leaks)"
    );
}

#[tokio::test]
async fn test_owned_read_guard_filter_map_memory_leak() {
    // Test OwnedRwLockReadGuard::filter_map memory management

    // Test success case
    {
        let rwlock = Arc::new(RwLock::new(Some(29u32)));
        let weak_ref: Weak<RwLock<Option<u32>>> = Arc::downgrade(&rwlock);

        {
            let read_guard = rwlock.clone().read_owned().await;
            let mapped_guard = OwnedRwLockReadGuard::filter_map(read_guard, |data| data.as_ref())
                .expect("filter_map should succeed for Some(_) value");
            assert_eq!(*mapped_guard, 29);
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "expected Arc to be dropped after filter_map success"
        );
    }

    // Test failure case
    {
        let rwlock = Arc::new(RwLock::new(None::<u32>));
        let weak_ref: Weak<RwLock<Option<u32>>> = Arc::downgrade(&rwlock);

        {
            let read_guard = rwlock.clone().read_owned().await;
            let result = OwnedRwLockReadGuard::filter_map(read_guard, |data| data.as_ref());
            assert!(result.is_err(), "filter_map should have failed for None");
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "expected Arc to be dropped after filter_map failure"
        );
    }
}

#[tokio::test]
async fn test_owned_mapped_read_guard_map_memory_leak() {
    // Test OwnedMappedRwLockReadGuard::map memory management
    let rwlock = Arc::new(RwLock::new("test".to_string()));
    let weak_ref: Weak<RwLock<String>> = Arc::downgrade(&rwlock);

    {
        let read_guard = rwlock.clone().read_owned().await;
        let mapped_guard1 = OwnedRwLockReadGuard::map(read_guard, |s| s);
        let mapped_guard2 = OwnedMappedRwLockReadGuard::map(mapped_guard1, |s| s.as_str());
        assert_eq!(&*mapped_guard2, "test");
    }

    drop(rwlock);
    assert!(
        weak_ref.upgrade().is_none(),
        "Memory leak detected: Arc was not deallocated"
    );
}

#[tokio::test]
async fn test_owned_mapped_read_guard_filter_map_memory_leak() {
    // Test OwnedMappedRwLockReadGuard::filter_map memory management

    // Test success case
    {
        let rwlock = Arc::new(RwLock::new(Some(29u32)));
        let weak_ref: Weak<RwLock<Option<u32>>> = Arc::downgrade(&rwlock);

        {
            let read_guard = rwlock.clone().read_owned().await;
            let mapped_guard1 = OwnedRwLockReadGuard::map(read_guard, |data| data);
            let mapped_guard2 =
                OwnedMappedRwLockReadGuard::filter_map(mapped_guard1, |opt| opt.as_ref())
                    .expect("filter_map should succeed for Some(_) value");
            assert_eq!(*mapped_guard2, 29);
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "expected Arc to be dropped after filter_map success"
        );
    }

    // Test failure case
    {
        let rwlock = Arc::new(RwLock::new(None::<u32>));
        let weak_ref: Weak<RwLock<Option<u32>>> = Arc::downgrade(&rwlock);

        {
            let read_guard = rwlock.clone().read_owned().await;
            let mapped_guard1 = OwnedRwLockReadGuard::map(read_guard, |data| data);
            let result = OwnedMappedRwLockReadGuard::filter_map(mapped_guard1, |opt| opt.as_ref());
            assert!(result.is_err(), "filter_map should have failed for None");
        }

        drop(rwlock);
        assert!(
            weak_ref.upgrade().is_none(),
            "Memory leak detected on filter_map failure"
        );
    }
}
