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
* [**WaitGroup**](https://docs.rs/mea/*/mea/waitgroup/struct.WaitGroup.html): A synchronization primitive that allows waiting for multiple tasks to complete.

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
