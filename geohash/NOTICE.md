# Attribution

This directory is a **fork** of the [`geohash`](https://github.com/georust/geohash)
crate at version **0.13.1**, maintained by the [GeoRust](https://github.com/georust)
organisation.

    Copyright (c) 2016 Ning Sun <sunng@about.me>

Upstream is dual-licensed under **MIT OR Apache-2.0**, and this fork is
redistributed under the same terms. The upstream license texts are kept verbatim
alongside this file as `LICENSE-MIT` and `LICENSE-APACHE`.

The fork is vendored rather than pulled from crates.io because the published crate
cannot be built for a bare-metal RISC-V target without an allocator-backed `std`.

## Changes from upstream v0.13.1

- Unconditional `#![no_std]`. Upstream's default `std` feature is removed entirely;
  the crate depends only on `core` plus `alloc` for `String`.
- Floating-point operations that upstream takes from `std` are provided by
  [`libm`](https://crates.io/crates/libm), and Euclidean division/remainder by
  [`num-traits`](https://crates.io/crates/num-traits) (`num_traits::ops::euclid`).
- `geo-types` is used with `default-features = false`.
- Upstream's test suite was dropped when the fork was taken. A replacement suite
  covering encode/decode round-trips, the eight neighbour directions and the error
  paths lives in `src/core.rs` and `src/neighbors.rs`.

No changes were made to the geohash algorithm itself; encoded output is identical
to upstream for the same input.
