# Mea

Mea (Made easy async) provides async utilities that are runtime agnostic.

## no_std

For `no_std` support, disable default features and add the `critical-section` feature:

```toml
[dependencies]
mea = { version = "0.1", default-features = false, features = ["critical-section"] }
```
