# Make Easy Async (Mea)

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MSRV 1.75][msrv-badge]](https://www.whatrustisit.com)
[![Apache 2.0 licensed][license-badge]][license-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/mea.svg
[crates-url]: https://crates.io/crates/mea
[docs-badge]: https://docs.rs/mea/badge.svg
[msrv-badge]: https://img.shields.io/badge/MSRV-1.75-green?logo=rust
[docs-url]: https://docs.rs/mea
[license-badge]: https://img.shields.io/crates/l/mea
[license-url]: LICENSE
[actions-badge]: https://github.com/tisonkun/mea/actions/workflows/ci.yml/badge.svg
[actions-url]: https://github.com/tisonkun/mea/actions/workflows/ci.yml

## Overview

Mea provides async utilities that are runtime agnostic.

* [Latch](https://docs.rs/mea/*/mea/latch/struct.Latch.html)
* [Semaphore](https://docs.rs/mea/*/mea/semaphore/struct.Semaphore.html)
* [WaitGroup](https://docs.rs/mea/*/mea/waitgroup/struct.WaitGroup.html)

## Usage

### Default Features

Add the dependency to your `Cargo.toml` via:

```toml
mea = { version = "<version>" }
```

### `no_std`

By default, Mea enables the `std` feature and thus depends on the Rust standard library. To use Mea in a
`no_std` Rust environment, simply disable the default features:

```toml
mea = { version = "<version>", default-features = false }
```

## Minimum Rust version policy

This crate is built against the latest stable release, and its minimum supported rustc version is 1.75.0.

The policy is that the minimum Rust version required to use this crate can be increased in minor version updates. For example, if Mea 1.0 requires Rust 1.20.0, then Mea 1.0.z for all values of z will also require Rust 1.20.0 or newer. However, Mea 1.y for y > 0 may require a newer minimum version of Rust.

## License

This project is licensed under [Apache License, Version 2.0](LICENSE).
