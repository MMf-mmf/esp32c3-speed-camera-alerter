//! # Geohash
//!
//! Geohash algorithm implementation in Rust. It encodes/decodes a
//! longitude-latitude tuple into/from a hashed string. You can find
//! more about geohash algorithm on [Wikipedia](https://en.wikipedia.org/wiki/Geohash)
//!
//! This is a `no_std` fork; see `NOTICE.md` for what differs from upstream.
//!
//! ## Usage
//! ```rust
//! use geohash::{encode, decode, neighbor, Direction, Coord};
//!
//! // encode a coordinate
//! let c = Coord { x: 112.5584f64, y: 37.8324f64 };
//! let encoded = encode(c, 9usize).unwrap();
//! assert_eq!(encoded, "ww8p1r4t8");
//!
//! // decode a geohash
//! let (c, _lon_err, _lat_err) = decode("ww8p1r4t8").unwrap();
//!
//! // find a neighboring hash
//! let sw = neighbor("ww8p1r4t8", Direction::SW).unwrap();
//! ```

#![doc(html_root_url = "https://docs.rs/geohash/")]
#![no_std]

// The test harness links std; the crate itself never does.
#[cfg(test)]
extern crate std;

mod core;
mod error;
mod neighbors;

pub use crate::core::{decode, decode_bbox, encode, neighbor, neighbors};
pub use crate::error::GeohashError;
pub use crate::neighbors::{Direction, Neighbors};
pub use geo_types::{Coord, Rect};
