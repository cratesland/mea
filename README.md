# Make Easy Async (Mea)

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MSRV 1.75][msrv-badge]](https://www.whatrustisit.com)
[![Apache 2.0 licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/mea.svg
[crates-url]: https://crates.io/crates/mea
[docs-badge]: https://docs.rs/mea/badge.svg
[docs-url]: https://docs.rs/mea
[msrv-badge]: https://img.shields.io/badge/MSRV-1.75-green?logo=rust
[license-badge]: https://img.shields.io/crates/l/mea
[license-url]: LICENSE
[actions-badge]: https://github.com/tisonkun/mea/actions/workflows/ci.yml/badge.svg
[actions-url]: https://github.com/tisonkun/mea/actions/workflows/ci.yml

## Overview

Mea (Make Easy Async) is a runtime-agnostic library providing essential synchronization primitives for asynchronous Rust programming. The library offers a collection of well-tested, efficient synchronization tools that work with any async runtime.

## Features

* **Barrier** - A synchronization point where multiple tasks can wait until all participants arrive
* **Latch** - A single-use barrier that allows one or more tasks to wait until a signal is given
* **Mutex** - A mutual exclusion primitive for protecting shared data
* **Semaphore** - A synchronization primitive that controls access to a shared resource
* **WaitGroup** - A synchronization primitive that allows waiting for multiple tasks to complete

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
mea = "0.0.5"
```

## Usage Examples

### Barrier

```rust
use mea::barrier::Barrier;
use std::sync::Arc;

async fn example() {
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();

    for i in 0..3 {
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            println!("Task {} before barrier", i);
            let is_leader = barrier.wait().await;
            println!("Task {} after barrier (leader: {})", i, is_leader);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}
```

### WaitGroup

```rust
use mea::waitgroup::WaitGroup;
use std::time::Duration;

async fn example() {
    let wg = WaitGroup::new();
    let mut handles = Vec::new();

    for i in 0..3 {
        let wg = wg.clone();
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("Task {} completed", i);
            // Wait until all tasks have finished
            wg.await
        }));
    }

    // Wait for all tasks to complete
    wg.await;
    println!("All tasks completed");
}
```

### Mutex

```rust
use mea::mutex::Mutex;
use std::sync::Arc;

async fn example() {
    let mutex = Arc::new(Mutex::new(0));
    let mut handles = Vec::new();

    for i in 0..3 {
        let mutex = mutex.clone();
        handles.push(tokio::spawn(async move {
            let mut lock = mutex.lock().await;
            *lock += i;
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let final_value = mutex.lock().await;
    assert_eq!(*final_value, 3); // 0 + 1 + 2
}
```

### Semaphore

```rust
use mea::semaphore::Semaphore;
use std::sync::Arc;

struct ConnectionPool {
    sem: Arc<Semaphore>,
}

impl ConnectionPool {
    fn new(size: u32) -> Self {
        Self {
            sem: Arc::new(Semaphore::new(size)),
        }
    }

    async fn get_connection(&self) -> Connection {
        let _permit = self.sem.acquire(1).await;
        Connection {} // Acquire and return a connection
    }
}

struct Connection {}
```

## Runtime Agnostic

All synchronization primitives in this library are runtime-agnostic, meaning they can be used with any async runtime like Tokio, async-std, or others. This makes the library highly versatile and portable.

## Thread Safety

All types in this library implement `Send` and `Sync`, making them safe to share across thread boundaries. This is essential for concurrent programming where data needs to be accessed from multiple threads.

## Minimum Supported Rust Version (MSRV)

This crate is built against the latest stable release, and its minimum supported rustc version is 1.75.0.

The policy is that the minimum Rust version required to use this crate can be increased in minor version updates. For example, if Mea 1.0 requires Rust 1.20.0, then Mea 1.0.z for all values of z will also require Rust 1.20.0 or newer. However, Mea 1.y for y > 0 may require a newer minimum version of Rust.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under [Apache License, Version 2.0](LICENSE).
