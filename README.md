# Make Easy Async (Mea)

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MSRV 1.80][msrv-badge]](https://www.whatrustisit.com)
[![Apache 2.0 licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/mea.svg
[crates-url]: https://crates.io/crates/mea
[docs-badge]: https://docs.rs/mea/badge.svg
[docs-url]: https://docs.rs/mea
[msrv-badge]: https://img.shields.io/badge/MSRV-1.80-green?logo=rust
[license-badge]: https://img.shields.io/crates/l/mea
[license-url]: LICENSE
[actions-badge]: https://github.com/cratesland/mea/actions/workflows/ci.yml/badge.svg
[actions-url]: https://github.com/cratesland/mea/actions/workflows/ci.yml

## Overview

Mea (Make Easy Async) is a runtime-agnostic library providing essential synchronization primitives for asynchronous Rust programming. The library offers a collection of well-tested, efficient synchronization tools that work with any async runtime.

## Features

* [**Barrier**](https://docs.rs/mea/*/mea/barrier/struct.Barrier.html): A synchronization primitive that enables tasks to wait until all participants arrive.
* [**Condvar**](https://docs.rs/mea/*/mea/condvar/struct.Condvar.html): A condition variable that allows tasks to wait for a notification.
* [**Latch**](https://docs.rs/mea/*/mea/latch/struct.Latch.html): A synchronization primitive that allows one or more tasks to wait until a set of operations completes.
* [**Mutex**](https://docs.rs/mea/*/mea/mutex/struct.Mutex.html): A mutual exclusion primitive for protecting shared data.
* [**RwLock**](https://docs.rs/mea/*/mea/rwlock/struct.RwLock.html): A reader-writer lock that allows multiple readers or a single writer at a time.
* [**Semaphore**](https://docs.rs/mea/*/mea/semaphore/struct.Semaphore.html): A synchronization primitive that controls access to a shared resource.
* [**ShutdownSend & ShutdownRecv**](https://docs.rs/mea/*/mea/shutdown/): A composite synchronization primitive for managing shutdown signals.
* [**WaitGroup**](https://docs.rs/mea/*/mea/waitgroup/struct.WaitGroup.html): A synchronization primitive that allows waiting for multiple tasks to complete.
* [**oneshot::channel**](https://docs.rs/mea/*/mea/oneshot/index.html): A one-shot channel for sending a single value between tasks.

## Installation

Add the dependency to your `Cargo.toml` via:

```shell
cargo add mea
```

## Runtime Agnostic

All synchronization primitives in this library are runtime-agnostic, meaning they can be used with any async runtime like Tokio, async-std, or others. This makes the library highly versatile and portable.

## Thread Safety

All types in this library implement `Send` and `Sync`, making them safe to share across thread boundaries. This is essential for concurrent programming where data needs to be accessed from multiple threads.

## Minimum Supported Rust Version (MSRV)

This crate is built against the latest stable release, and its minimum supported rustc version is 1.80.0.

The policy is that the minimum Rust version required to use this crate can be increased in minor version updates. For example, if Mea 1.0 requires Rust 1.20.0, then Mea 1.0.z for all values of z will also require Rust 1.20.0 or newer. However, Mea 1.y for y > 0 may require a newer minimum version of Rust.

## License

This project is licensed under [Apache License, Version 2.0](LICENSE).

## History

This crate collects runtime-agnostic synchronization primitives from spare parts:

* **Barrier** is inspired by `std::sync::Barrier` and `tokio::sync::Barrier`, with a different implementation based on the internal `WaitSet` primitive.
* **Condvar** is inspired by `std::sync::Condvar` and `async_std::sync::Condvar`, with a different implementation based on the internal `Semaphore` primitive. Different from the async_std implementation, this condvar is fair.
* **Latch** is inspired by [`latches`](https://github.com/mirromutth/latches), with a different implementation based on the internal `CountdownState` primitive. No `wait` or `watch` method is provided, since it can be easily implemented by [composing delay futures](https://docs.rs/fastimer/*/fastimer/fn.timeout.html). No sync variant is provided, since it can be easily implemented with block_on of any runtime.
* **Mutex** is derived from `tokio::sync::Mutex`. No blocking method is provided, since it can be easily implemented with block_on of any runtime.
* **RwLock** is derived from `tokio::sync::RwLock`, but the `max_readers` can be any `usize` instead of `[0, u32::MAX >> 3]`. No blocking method is provided, since it can be easily implemented with block_on of any runtime.
* **Semaphore** is derived from `tokio::sync::Semaphore`, without `close` method since it is quite tricky to use. And thus, this semaphore doesn't have the limitation of max permits. Besides, new methods like `forget_exact` are added to fit the specific use case.
* **WaitGroup** is inspired by [`waitgroup-rs`](https://github.com/laizy/waitgroup-rs), with a different implementation based on the internal `CountdownState` primitive. It fixes the unsound issue as described [here](https://github.com/rust-lang/futures-rs/issues/2880#issuecomment-2333842804).
* **oneshot::channel** is derived from [`oneshot`](https://github.com/faern/oneshot), with significant simplifications since we need not support synchronized receiving functions.

Other parts are written from scratch.

NB. The `Mapped` variant of `Mutex` and `RwLock` is not implemented yet. [The rationale of this variant](https://github.com/rust-lang/libs-team/issues/260) is valid. Just I don't have time to review the implementation.

NB. The optimization considerations are different when implementing a sync primitive for sync code and async code. Generally speaking, once you have an async + runtime-agnostic implementation, you can immediately have a sync implementation by block_on any async runtime ([`pollster`](https://github.com/zesterer/pollster) is the most lightweight runtime that park the current thread). However, a sync-oriented implementation may leverage some platform-specific features to achieve better performance. This library is designed for async code, so it doesn't consider sync-oriented optimization. I often find libraries that try to provide both sync and async implementations end up with a clumsy API design. So I prefer to keep them separate.
