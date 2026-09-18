use crate::neighbors::Direction;
use crate::{Coord, GeohashError, Neighbors, Rect};
extern crate alloc;
use alloc::string::{String, ToString};
use libm::ldexp;
use num_traits::ops::euclid::Euclid;

// the alphabet for the base32 encoding used in geohashing
#[rustfmt::skip]
const BASE32_CODES: [char; 32] = [
    '0', '1', '2', '3', '4', '5', '6', '7',
    '8', '9', 'b', 'c', 'd', 'e', 'f', 'g',
    'h', 'j', 'k', 'm', 'n', 'p', 'q', 'r',
    's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

// array that is indexed into to get the value of a character in our base32 alphabet
#[rustfmt::skip]
const DECODER: [u8; 256] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0xff, 0x11, 0x12, 0xff, 0x13, 0x14, 0xff,
    0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
    0x1d, 0x1e, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,

    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

// bit shifting functions used in encoding and decoding

// spread takes a u32 and deposits its bits into the evenbit positions of a u64
#[inline]
fn spread(x: u32) -> u64 {
    let mut new_x = x as u64;
    new_x = (new_x | (new_x << 16)) & 0x0000ffff0000ffff;
    new_x = (new_x | (new_x << 8)) & 0x00ff00ff00ff00ff;
    new_x = (new_x | (new_x << 4)) & 0x0f0f0f0f0f0f0f0f;
    new_x = (new_x | (new_x << 2)) & 0x3333333333333333;
    new_x = (new_x | (new_x << 1)) & 0x5555555555555555;

    new_x
}

// spreads the inputs, then shifts the y input and does a bitwise or to fill the remaining bits in x
#[inline]
fn interleave(x: u32, y: u32) -> u64 {
    spread(x) | (spread(y) << 1)
}

// squashes the even bit positions of a u64 into a u32
#[inline]
fn squash(x: u64) -> u32 {
    let mut new_x = x & 0x5555555555555555;
    new_x = (new_x | (new_x >> 1)) & 0x3333333333333333;
    new_x = (new_x | (new_x >> 2)) & 0x0f0f0f0f0f0f0f0f;
    new_x = (new_x | (new_x >> 4)) & 0x00ff00ff00ff00ff;
    new_x = (new_x | (new_x >> 8)) & 0x0000ffff0000ffff;
    new_x = (new_x | (new_x >> 16)) & 0x00000000ffffffff;
    new_x as u32
}

// uses the squash function to create a 32 from the even bits
// then shifts the input right and squashes to create a u32 from the odd bits
#[inline]
fn deinterleave(x: u64) -> (u32, u32) {
    (squash(x), squash(x >> 1))
}

/// Encode a coordinate to a geohash with length `len`.
///
/// ### Examples
///
/// Encoding a coordinate to a length five geohash:
///
/// ```rust
/// let coord = geohash::Coord { x: -120.6623, y: 35.3003 };
///
/// let geohash_string = geohash::encode(coord, 5).expect("Invalid coordinate");
///
/// assert_eq!(geohash_string, "9q60y");
/// ```
///
/// Encoding a coordinate to a length ten geohash:
///
/// ```rust
/// let coord = geohash::Coord { x: -120.6623, y: 35.3003 };
///
/// let geohash_string = geohash::encode(coord, 10).expect("Invalid coordinate");
///
/// assert_eq!(geohash_string, "9q60y60rhs");
/// ```
pub fn encode(c: Coord<f64>, len: usize) -> Result<String, GeohashError> {
    let max_lat = 90f64;
    let min_lat = -90f64;
    let max_lon = 180f64;
    let min_lon = -180f64;

    if !(min_lon..=max_lon).contains(&c.x) || !(min_lat..=max_lat).contains(&c.y) {
        return Err(GeohashError::InvalidCoordinateRange(c));
    }

    if !(1..=12).contains(&len) {
        return Err(GeohashError::InvalidLength(len));
    }

    // divides the latitude by 180, then adds 1.5 to give a value between 1 and 2
    // then we take the first 32 bits of the significand as a u32
    let lat32 = ((c.y * 0.005555555555555556 + 1.5).to_bits() >> 20) as u32;
    // same as latitude, but a division by 360 instead of 180
    let lon32 = ((c.x * 0.002777777777777778 + 1.5).to_bits() >> 20) as u32;

    let mut interleaved_int = interleave(lat32, lon32);

    let mut out = String::with_capacity(len);
    // loop through and take the first 5 bits of the interleaved value ech iteration
    for _ in 0..len {
        // shifts so that the high 5 bits are now the low five bits, then masks to get their value
        let code = (interleaved_int >> 59) as usize & (0x1f);
        // uses that value to index into the array of base32 codes
        out.push(BASE32_CODES[code]);
        // shifts the interleaved bits left by 5, so we get the next 5 bits on the next iteration
        interleaved_int <<= 5;
    }
    Ok(out)
}

/// Decode geohash string into latitude, longitude
///
/// Parameters:
/// Geohash encoded `&str`
///
/// Returns:
/// A four-element tuple describes a bound box:
/// * min_lat
/// * max_lat
/// * min_lon
/// * max_lon
pub fn decode_bbox(hash_str: &str) -> Result<Rect<f64>, GeohashError> {
    let bits = hash_str.len() * 5;

    if hash_str.len() > 12 {
        return Err(GeohashError::InvalidHash(
            "Length of hash string greater than maximum allowed length".to_string(),
        ));
    }

    let mut int_hash: u64 = 0;
    for c in hash_str.bytes() {
        // getting the value from the array converts from the base32 alphabet to an integer value
        let hash_value = DECODER[c as usize];
        // this means that we have indexed into the position of an invalid character
        if hash_value == 0xff {
            return Err(GeohashError::InvalidHashCharacter(c as char));
        }
        // shift int_hash and deposit the newly decoded bits into its lowest bits
        int_hash <<= 5;
        int_hash |= hash_value as u64;
    }

    Ok(bbox_int_with_precision(int_hash, bits as u32))
}

fn decode_range(x: u32, r: f64) -> f64 {
    // f64 in the range 1 to 2 where 1 would represent -r and 2 would represent r
    let p = f64::from_bits(((x as u64) << 20) | (1023 << 52));
    // converts the value between 1 and 2 to a value between -r and r
    2.0 * r * (p - 1.0) - r
}

fn error_with_precision(bits: u32) -> (f64, f64) {
    let lat_bits = bits / 2;
    let long_bits = bits - lat_bits;

    // the ldexp(x, n) function is equivalent to x * 2^n but with better performance
    let lat_err = ldexp(180.0, -(lat_bits as i32));
    let long_err = ldexp(360.0, -(long_bits as i32));
    (lat_err, long_err)
}

fn bbox_int_with_precision(hash: u64, bits: u32) -> Rect<f64> {
    let full_hash = hash << (64 - bits);
    let (lat_int, long_int) = deinterleave(full_hash);
    let lat = decode_range(lat_int, 90.0);
    let long = decode_range(long_int, 180.0);
    let (lat_err, long_err) = error_with_precision(bits);

    Rect::new(
        Coord { x: long, y: lat },
        Coord {
            x: long + long_err,
            y: lat + lat_err,
        },
    )
}

/// Decode a geohash into a coordinate with some longitude/latitude error. The
/// return value is `(<coordinate>, <longitude error>, <latitude error>)`.
///
/// ### Examples
///
/// Decoding a length five geohash:
///
/// ```rust
/// let geohash_str = "9q60y";
///
/// let decoded = geohash::decode(geohash_str).expect("Invalid hash string");
///
/// assert_eq!(
///     decoded,
///     (
///         geohash::Coord {
///             x: -120.65185546875,
///             y: 35.31005859375,
///         },
///         0.02197265625,
///         0.02197265625,
///     ),
/// );
/// ```
///
/// Decoding a length ten geohash:
///
/// ```rust
/// let geohash_str = "9q60y60rhs";
///
/// let decoded = geohash::decode(geohash_str).expect("Invalid hash string");
///
/// assert_eq!(
///     decoded,
///     (
///         geohash::Coord {
///             x: -120.66229999065399,
///             y: 35.300298035144806,
///         },
///         0.000005364418029785156,
///         0.000002682209014892578,
///     ),
/// );
/// ```
pub fn decode(hash_str: &str) -> Result<(Coord<f64>, f64, f64), GeohashError> {
    let rect = decode_bbox(hash_str)?;
    let c0 = rect.min();
    let c1 = rect.max();
    Ok((
        Coord {
            x: (c0.x + c1.x) / 2f64,
            y: (c0.y + c1.y) / 2f64,
        },
        (c1.x - c0.x) / 2f64,
        (c1.y - c0.y) / 2f64,
    ))
}

/// Find neighboring geohashes for the given geohash and direction.
///
/// ### Examples
///
/// ```
/// # use geohash::Direction;
/// # fn main() {
/// let geohash_str = "9q60y60rhs";
///
/// let neighbor = geohash::neighbor(geohash_str, Direction::N).expect("Invalid hash string");
///
/// assert_eq!(neighbor, "9q60y60rht".to_owned());
/// # }
/// ```
pub fn neighbor(hash_str: &str, direction: Direction) -> Result<String, GeohashError> {
    let (coord, lon_err, lat_err) = decode(hash_str)?;
    let (dlat, dlng) = direction.to_tuple();
    // Called as an explicit trait call rather than `x.rem_euclid(..)`: under
    // `cargo test` the crate links std, whose inherent `f64::rem_euclid` would
    // otherwise shadow this and have the tests exercise a different path from
    // the no_std build that actually ships.
    let neighbor_coord = Coord {
        x: Euclid::rem_euclid(&((coord.x + 2f64 * lon_err.abs() * dlng) + 180.0), &360.0) - 180.0,
        y: Euclid::rem_euclid(&((coord.y + 2f64 * lat_err.abs() * dlat) + 90.0), &180.0) - 90.0,
    };
    encode(neighbor_coord, hash_str.len())
}

/// Find all neighboring geohashes for the given geohash.
///
/// ### Examples
///
/// ```
/// let geohash_str = "9q60y60rhs";
///
/// let neighbors = geohash::neighbors(geohash_str).expect("Invalid hash string");
///
/// assert_eq!(
///     neighbors,
///     geohash::Neighbors {
///         n: "9q60y60rht".to_owned(),
///         ne: "9q60y60rhv".to_owned(),
///         e: "9q60y60rhu".to_owned(),
///         se: "9q60y60rhg".to_owned(),
///         s: "9q60y60rhe".to_owned(),
///         sw: "9q60y60rh7".to_owned(),
///         w: "9q60y60rhk".to_owned(),
///         nw: "9q60y60rhm".to_owned(),
///     }
/// );
/// ```
pub fn neighbors(hash_str: &str) -> Result<Neighbors, GeohashError> {
    Ok(Neighbors {
        sw: neighbor(hash_str, Direction::SW)?,
        s: neighbor(hash_str, Direction::S)?,
        se: neighbor(hash_str, Direction::SE)?,
        w: neighbor(hash_str, Direction::W)?,
        e: neighbor(hash_str, Direction::E)?,
        nw: neighbor(hash_str, Direction::NW)?,
        n: neighbor(hash_str, Direction::N)?,
        ne: neighbor(hash_str, Direction::NE)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Direction;

    // A coordinate in San Luis Obispo, CA, and its known encodings. These are the
    // vectors the upstream crate documents, kept here so a regression in the
    // bit-twiddling shows up as a test failure rather than a wrong alert.
    const SLO: Coord<f64> = Coord {
        x: -120.6623,
        y: 35.3003,
    };

    #[test]
    fn encode_matches_known_vectors() {
        assert_eq!(encode(SLO, 5).unwrap(), "9q60y");
        assert_eq!(encode(SLO, 10).unwrap(), "9q60y60rhs");
        assert_eq!(
            encode(
                Coord {
                    x: 112.5584,
                    y: 37.8324
                },
                9
            )
            .unwrap(),
            "ww8p1r4t8"
        );
    }

    #[test]
    fn encode_honours_requested_length() {
        for len in 1..=12 {
            assert_eq!(encode(SLO, len).unwrap().len(), len);
        }
    }

    #[test]
    fn encode_produces_prefixes_of_longer_hashes() {
        // A shorter geohash must be a prefix of a longer one for the same point.
        // The firmware relies on this when it searches at precision 7.
        let long = encode(SLO, 12).unwrap();
        for len in 1..=12 {
            assert_eq!(encode(SLO, len).unwrap(), long[..len]);
        }
    }

    #[test]
    fn decode_matches_known_vector() {
        let (coord, lon_err, lat_err) = decode("9q60y").unwrap();
        assert_eq!(coord.x, -120.65185546875);
        assert_eq!(coord.y, 35.31005859375);
        assert_eq!(lon_err, 0.02197265625);
        assert_eq!(lat_err, 0.02197265625);
    }

    #[test]
    fn round_trip_stays_within_reported_error() {
        let points = [
            SLO,
            Coord { x: 0.0, y: 0.0 },
            Coord {
                x: -87.70453,
                y: 41.994995,
            }, // a Chicago speed camera from the shipped dataset
            Coord {
                x: -73.901978,
                y: 40.915523,
            },
            Coord {
                x: 179.9999,
                y: -89.9999,
            },
        ];
        for p in points {
            for len in 1..=12 {
                let hash = encode(p, len).unwrap();
                let (decoded, lon_err, lat_err) = decode(&hash).unwrap();
                assert!(
                    (decoded.x - p.x).abs() <= lon_err,
                    "lon {} outside +/-{} of {} at len {}",
                    decoded.x,
                    lon_err,
                    p.x,
                    len
                );
                assert!(
                    (decoded.y - p.y).abs() <= lat_err,
                    "lat {} outside +/-{} of {} at len {}",
                    decoded.y,
                    lat_err,
                    p.y,
                    len
                );
            }
        }
    }

    #[test]
    fn decode_bbox_contains_the_encoded_point() {
        let hash = encode(SLO, 7).unwrap();
        let rect = decode_bbox(&hash).unwrap();
        assert!(rect.min().x <= SLO.x && SLO.x <= rect.max().x);
        assert!(rect.min().y <= SLO.y && SLO.y <= rect.max().y);
    }

    #[test]
    fn encode_rejects_coordinates_outside_the_world() {
        for bad in [
            Coord { x: 0.0, y: 90.001 },
            Coord { x: 0.0, y: -90.001 },
            Coord { x: 180.001, y: 0.0 },
            Coord {
                x: -180.001,
                y: 0.0,
            },
        ] {
            assert!(matches!(
                encode(bad, 7),
                Err(GeohashError::InvalidCoordinateRange(_))
            ));
        }
    }

    #[test]
    fn encode_rejects_lengths_outside_one_to_twelve() {
        assert!(matches!(
            encode(SLO, 0),
            Err(GeohashError::InvalidLength(0))
        ));
        assert!(matches!(
            encode(SLO, 13),
            Err(GeohashError::InvalidLength(13))
        ));
    }

    #[test]
    fn decode_rejects_characters_outside_the_base32_alphabet() {
        // 'a', 'i', 'l' and 'o' are deliberately absent from the geohash alphabet.
        for bad in ["9q60ya", "9q60yi", "9q60yl", "9q60yo", "9q60y!"] {
            assert!(
                matches!(decode(bad), Err(GeohashError::InvalidHashCharacter(_))),
                "expected {bad} to be rejected"
            );
        }
    }

    #[test]
    fn decode_rejects_overlong_hashes() {
        assert!(matches!(
            decode("9q60y60rhs123"),
            Err(GeohashError::InvalidHash(_))
        ));
    }

    #[test]
    fn neighbor_matches_known_vector() {
        assert_eq!(neighbor("9q60y60rhs", Direction::N).unwrap(), "9q60y60rht");
    }

    #[test]
    fn neighbors_matches_known_vectors() {
        let n = neighbors("9q60y60rhs").unwrap();
        assert_eq!(n.n, "9q60y60rht");
        assert_eq!(n.ne, "9q60y60rhv");
        assert_eq!(n.e, "9q60y60rhu");
        assert_eq!(n.se, "9q60y60rhg");
        assert_eq!(n.s, "9q60y60rhe");
        assert_eq!(n.sw, "9q60y60rh7");
        assert_eq!(n.w, "9q60y60rhk");
        assert_eq!(n.nw, "9q60y60rhm");
    }

    #[test]
    fn opposite_directions_return_to_the_origin() {
        let origin = encode(SLO, 9).unwrap();
        for (there, back) in [
            (Direction::N, Direction::S),
            (Direction::E, Direction::W),
            (Direction::NE, Direction::SW),
            (Direction::SE, Direction::NW),
        ] {
            let away = neighbor(&origin, there).unwrap();
            assert_eq!(neighbor(&away, back).unwrap(), origin);
        }
    }

    #[test]
    fn neighbors_keep_the_precision_of_their_origin() {
        for len in 1..=12 {
            let origin = encode(SLO, len).unwrap();
            let n = neighbors(&origin).unwrap();
            for hash in [n.n, n.ne, n.e, n.se, n.s, n.sw, n.w, n.nw] {
                assert_eq!(hash.len(), len);
            }
        }
    }

    #[test]
    fn neighbors_wrap_across_the_antimeridian_and_the_poles() {
        // Not asserting specific hashes here -- the point is that these do not
        // error or produce a coordinate outside the valid range, which is what
        // the firmware's nine-cell search would trip over.
        for edge in [
            Coord { x: 179.999, y: 0.0 },
            Coord {
                x: -179.999,
                y: 0.0,
            },
            Coord { x: 0.0, y: 89.999 },
            Coord { x: 0.0, y: -89.999 },
        ] {
            let origin = encode(edge, 7).unwrap();
            let n = neighbors(&origin).expect("neighbours at the edge of the world");
            for hash in [n.n, n.ne, n.e, n.se, n.s, n.sw, n.w, n.nw] {
                assert_eq!(hash.len(), 7);
                decode(&hash).expect("every neighbour decodes");
            }
        }
    }
}
