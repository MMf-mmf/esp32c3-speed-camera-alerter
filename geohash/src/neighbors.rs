extern crate alloc;
use alloc::string::String;

#[derive(Debug, Clone, PartialEq)]
pub struct Neighbors {
    pub sw: String,
    pub s: String,
    pub se: String,
    pub w: String,
    pub e: String,
    pub nw: String,
    pub n: String,
    pub ne: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// North
    N,
    /// North-east
    NE,
    /// Eeast
    E,
    /// South-east
    SE,
    /// South
    S,
    /// South-west
    SW,
    /// West
    W,
    /// North-west
    NW,
}

impl Direction {
    pub fn to_tuple(self) -> (f64, f64) {
        match self {
            Direction::SW => (-1f64, -1f64),
            Direction::S => (-1f64, 0f64),
            Direction::SE => (-1f64, 1f64),
            Direction::W => (0f64, -1f64),
            Direction::E => (0f64, 1f64),
            Direction::NW => (1f64, -1f64),
            Direction::N => (1f64, 0f64),
            Direction::NE => (1f64, 1f64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_offsets_are_a_unit_box() {
        // (dlat, dlng), north and east positive.
        assert_eq!(Direction::N.to_tuple(), (1.0, 0.0));
        assert_eq!(Direction::NE.to_tuple(), (1.0, 1.0));
        assert_eq!(Direction::E.to_tuple(), (0.0, 1.0));
        assert_eq!(Direction::SE.to_tuple(), (-1.0, 1.0));
        assert_eq!(Direction::S.to_tuple(), (-1.0, 0.0));
        assert_eq!(Direction::SW.to_tuple(), (-1.0, -1.0));
        assert_eq!(Direction::W.to_tuple(), (0.0, -1.0));
        assert_eq!(Direction::NW.to_tuple(), (1.0, -1.0));
    }

    #[test]
    fn opposite_directions_cancel() {
        for (a, b) in [
            (Direction::N, Direction::S),
            (Direction::E, Direction::W),
            (Direction::NE, Direction::SW),
            (Direction::SE, Direction::NW),
        ] {
            let (alat, alng) = a.to_tuple();
            let (blat, blng) = b.to_tuple();
            assert_eq!((alat + blat, alng + blng), (0.0, 0.0));
        }
    }
}
