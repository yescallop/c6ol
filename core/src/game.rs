//! Connect6 game logic, record, and serialization.

mod bit;

#[cfg(test)]
mod tests;

use bytes::{Buf, BufMut};
use bytes_varint::{VarIntSupport, VarIntSupportMut};
use std::{
    collections::HashMap,
    iter,
    ops::{Add, Index, IndexMut, Sub},
};

use bit::{BitReader, BitWriter};

/// A direction on the board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// North, with a unit vector of `(0, -1)`.
    North = 0,
    /// Northeast, with a unit vector of `(1, -1)`.
    Northeast = 1,
    /// East, with a unit vector of `(1, 0)`.
    East = 2,
    /// Southeast, with a unit vector of `(1, 1)`.
    Southeast = 3,
    /// South, with a unit vector of `(0, 1)`.
    South = 4,
    /// Southwest, with a unit vector of `(-1, 1)`.
    Southwest = 5,
    /// West, with a unit vector of `(-1, 0)`.
    West = 6,
    /// Northwest, with a unit vector of `(-1, -1)`.
    Northwest = 7,
}

impl Direction {
    /// List of all pairs of opposite directions.
    pub const OPPOSITE_PAIRS: [(Self, Self); 4] = [
        (Self::North, Self::South),
        (Self::Northeast, Self::Southwest),
        (Self::East, Self::West),
        (Self::Southeast, Self::Northwest),
    ];

    /// Creates a direction from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        Some(match n {
            0 => Self::North,
            1 => Self::Northeast,
            2 => Self::East,
            3 => Self::Southeast,
            4 => Self::South,
            5 => Self::Southwest,
            6 => Self::West,
            7 => Self::Northwest,
            _ => return None,
        })
    }

    /// Creates a direction from a unit vector.
    #[must_use]
    pub fn from_unit_vec(dx: i16, dy: i16) -> Option<Self> {
        Some(match (dx, dy) {
            (0, -1) => Self::North,
            (1, -1) => Self::Northeast,
            (1, 0) => Self::East,
            (1, 1) => Self::Southeast,
            (0, 1) => Self::South,
            (-1, 1) => Self::Southwest,
            (-1, 0) => Self::West,
            (-1, -1) => Self::Northwest,
            _ => return None,
        })
    }

    /// Returns the unit vector in this direction.
    #[must_use]
    pub fn unit_vec(self) -> (i16, i16) {
        match self {
            Self::North => (0, -1),
            Self::Northeast => (1, -1),
            Self::East => (1, 0),
            Self::Southeast => (1, 1),
            Self::South => (0, 1),
            Self::Southwest => (-1, 1),
            Self::West => (-1, 0),
            Self::Northwest => (-1, -1),
        }
    }
}

/// Maps an integer to a natural number.
fn zigzag_encode(n: i16) -> u16 {
    ((n << 1) ^ (n >> 15)) as u16
}

/// Maps a natural number to an integer (undoes `zigzag_encode`).
fn zigzag_decode(n: u16) -> i16 {
    ((n >> 1) ^ (n & 1).wrapping_neg()) as i16
}

/// Maps two natural numbers to one.
fn szudzik_pair(x: u16, y: u16) -> u32 {
    let (x, y) = (x as u32, y as u32);
    if x < y { y * y + x } else { x * x + x + y }
}

/// Maps one natural number to two (undoes `szudzik_pair`).
fn szudzik_unpair(z: u32) -> (u16, u16) {
    let s = z.isqrt();
    let t = z - s * s;
    if t < s {
        (t as u16, s as u16)
    } else {
        (s as u16, (t - s) as u16)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OrbitKind {
    Central,
    Axial,
    Diagonal,
    General,
}

/// A 2D point with integer coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Point {
    /// The east-west coordinate.
    pub x: i16,
    /// The north-south coordinate.
    pub y: i16,
}

impl Point {
    /// A point with zero coordinates.
    pub const ZERO: Self = Self { x: 0, y: 0 };

    /// Creates a point with the given coordinates.
    #[must_use]
    pub fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }

    /// Maps the point to a natural number.
    #[must_use]
    pub fn index(self) -> u32 {
        szudzik_pair(zigzag_encode(self.x), zigzag_encode(self.y))
    }

    /// Maps a natural number to a point (undoes `old_index`).
    #[must_use]
    pub fn from_index(i: u32) -> Self {
        let (x, y) = szudzik_unpair(i);
        Self::new(zigzag_decode(x), zigzag_decode(y))
    }

    /// Maps the point to a natural number.
    #[must_use]
    pub fn new_index(self) -> Option<u32> {
        let (x, y) = (self.x as i32, self.y as i32);
        let u = x.unsigned_abs();
        let v = y.unsigned_abs();

        // Shell index
        let s = u.max(v);
        if s == 0 {
            return Some(0);
        }
        if s > 0x7fff {
            return None;
        }

        // Base index, the number of points in all inner shells
        let base = (2 * s - 1).pow(2);
        // Relative index within the quarter-shell, equal to pair(u, v) - s^2
        let k = if u <= v { 2 * u } else { 2 * v + 1 };
        let group_offset = match k {
            0 => 0, // (0, s)
            1 => 2, // (s, 0)
            _ => 4 * k - 4,
        };
        let sign_offset = match k {
            0 => (y > 0) as u32, // (0, -s), (0, s)
            1 => (x > 0) as u32, // (-s, 0), (s, 0)
            _ => (x > 0) as u32 * 2 + (y > 0) as u32,
        };

        Some(base + group_offset + sign_offset)
    }

    /// Maps a natural number to a point (undoes `index`).
    #[must_use]
    pub fn from_new_index(n: u32) -> Option<Self> {
        if n == 0 {
            return Some(Self::new(0, 0));
        }
        let s = n.isqrt().div_ceil(2);
        if s > 0x7fff {
            return None;
        }

        let base = (2 * s - 1).pow(2);
        let r = n - base;

        let s = s as i16;
        Some(match r {
            0 => Self::new(0, -s),
            1 => Self::new(0, s),
            2 => Self::new(-s, 0),
            3 => Self::new(s, 0),
            _ => {
                let k = r / 4 + 1;
                let h = (k / 2) as i16;
                let (u, v) = if k & 1 == 0 { (h, s) } else { (s, h) };

                let x = if r & 2 == 0 { -u } else { u };
                let y = if r & 1 == 0 { -v } else { v };
                Self::new(x, y)
            }
        })
    }

    /// Maps the point to a natural number, with centrosymmetric
    /// points mapped to the same number.
    #[must_use]
    pub fn sym_index(self) -> Option<u32> {
        let (x, y) = (self.x as i32, self.y as i32);
        let u = x.unsigned_abs();
        let v = y.unsigned_abs();

        let s = u.max(v);
        if s == 0 {
            return Some(0);
        }
        if s > 0x7fff {
            return None;
        }

        let base = 2 * s * s - 2 * s + 1;
        let k = if u <= v { 2 * u } else { 2 * v + 1 };
        let group_offset = match k {
            0 => 0, // (0, s)
            1 => 1, // (s, 0)
            _ => 2 * k - 2,
        };
        let sign = if (x, y) >= (0, 0) { y > 0 } else { y < 0 };
        let sign_offset = (k >= 2 && sign) as u32;

        Some(base + group_offset + sign_offset)
    }

    /// Maps a natural number to a point, the lexicographically
    /// less one in a centrosymmetric group (undoes `sym_index`).
    #[must_use]
    pub fn from_sym_index(n: u32) -> Option<Self> {
        const MAX_N: u32 = 2 * 0x8000 * 0x8000 - 2 * 0x8000;

        if n == 0 {
            return Some(Self::new(0, 0));
        }
        if n > MAX_N {
            return None;
        }

        let s = (2 * n - 1).isqrt().div_ceil(2);
        let base = 2 * s * s - 2 * s + 1;
        let r = n - base;

        let s = s as i16;
        Some(match r {
            0 => Self::new(0, -s),
            1 => Self::new(-s, 0),
            _ => {
                let k = r / 2 + 1;
                let h = (k / 2) as i16;
                let (u, v) = if k & 1 == 0 { (h, s) } else { (s, h) };

                let x = u;
                let y = if r & 1 == 0 { -v } else { v };
                Self::new(-x, -y)
            }
        })
    }

    /// Returns the adjacent point in the given direction,
    /// or `None` on overflow.
    #[must_use]
    pub fn adjacent(self, dir: Direction) -> Option<Self> {
        let (dx, dy) = dir.unit_vec();
        Some(Self::new(self.x.checked_add(dx)?, self.y.checked_add(dy)?))
    }

    /// Returns an iterator of adjacent points in the given direction,
    /// which stops on overflow.
    pub fn adjacent_iter(self, dir: Direction) -> impl Iterator<Item = Self> {
        let mut cur = self;
        let (dx, dy) = dir.unit_vec();

        iter::from_fn(move || {
            cur = Self::new(cur.x.checked_add(dx)?, cur.y.checked_add(dy)?);
            Some(cur)
        })
    }

    /// Calculates the midpoint of two points, rounding towards negative infinity.
    #[must_use]
    pub fn midpoint_floor(self, other: Self) -> Self {
        let x = (self.x as i32 + other.x as i32) >> 1;
        let y = (self.y as i32 + other.y as i32) >> 1;
        Self::new(x as i16, y as i16)
    }

    /// Encodes the point to a buffer.
    pub fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u32_varint(self.index());
    }

    /// Decodes a point from a buffer.
    #[must_use]
    pub fn decode(buf: &mut &[u8]) -> Option<Self> {
        buf.try_get_u32_varint().ok().map(Self::from_index)
    }

    pub fn encode_d4(self, block_sizes: &[u8], writer: &mut BitWriter<'_>, centrosymmetric: bool) {
        let (x, y) = (self.x as i32, self.y as i32);
        let xa = x.unsigned_abs();
        let ya = y.unsigned_abs();
        let (u, v) = if xa < ya { (xa, ya) } else { (ya, xa) };

        let orbit = v * (v + 1) / 2 + u;
        writer.write_u32_varint_with_sizes(orbit, block_sizes);

        let (orbit_kind, mut sector);

        if v == 0 {
            orbit_kind = OrbitKind::Central;
            // (0,0)
            sector = 0;
        } else if u == 0 {
            orbit_kind = OrbitKind::Axial;
            // (-,0) -> (0,-) -> (0,+) -> (+,0)
            sector = ((x > 0 || y > 0) as u8) << 1 | ((x > 0 || y < 0) as u8);
        } else if u == v {
            orbit_kind = OrbitKind::Diagonal;
            // (-,-) -> (-,+) -> (+,-) -> (+,+)
            sector = ((x > 0) as u8) << 1 | ((y > 0) as u8);
        } else {
            orbit_kind = OrbitKind::General;
            // (-v,-u) -> (-v,u) -> (-u,-v) -> (-u,v)
            // -> (u,-v) -> (u,v) -> (v,-u) -> (v,u)
            sector = ((x > 0) as u8) << 2 | (((x > 0) ^ (xa < ya)) as u8) << 1 | ((y > 0) as u8);
        };

        let sector_bits = match orbit_kind {
            OrbitKind::Central => 0,
            OrbitKind::Axial | OrbitKind::Diagonal => {
                if centrosymmetric {
                    sector = sector.min(3 - sector);
                }
                2 - centrosymmetric as u8
            }
            OrbitKind::General => {
                if centrosymmetric {
                    sector = sector.min(7 - sector);
                }
                3 - centrosymmetric as u8
            }
        };
        writer.write(sector, sector_bits);
    }

    pub fn decode_d4(
        orbit: u32,
        reader: &mut BitReader<'_, '_>,
        centrosymmetric: bool,
    ) -> Option<Self> {
        const MAX_ORBIT: u32 = 0x7ffe * (0x7ffe + 1) / 2 + 0x7ffe;

        if orbit > MAX_ORBIT {
            return None;
        }

        let v = ((8 * orbit + 1).isqrt() - 1) / 2;
        let u = orbit - v * (v + 1) / 2;
        let (u, v) = (u as i16, v as i16);

        let orbit_kind = if v == 0 {
            OrbitKind::Central
        } else if u == 0 {
            OrbitKind::Axial
        } else if u == v {
            OrbitKind::Diagonal
        } else {
            OrbitKind::General
        };

        let sector_bits = match orbit_kind {
            OrbitKind::Central => 0,
            OrbitKind::Axial | OrbitKind::Diagonal => 2 - centrosymmetric as u8,
            OrbitKind::General => 3 - centrosymmetric as u8,
        };
        let sector = reader.read(sector_bits)?;

        Some(match orbit_kind {
            OrbitKind::Central => Self::ZERO,
            OrbitKind::Axial => {
                let val = if sector & 2 == 0 { -v } else { v };
                let swap = (sector ^ (sector >> 1)) & 1 == 0;
                if swap {
                    Self::new(val, 0)
                } else {
                    Self::new(0, val)
                }
            }
            OrbitKind::Diagonal => {
                let x = if sector & 2 == 0 { -v } else { v };
                let y = if sector & 1 == 0 { -v } else { v };
                Self::new(x, y)
            }
            OrbitKind::General => {
                let swap = ((sector >> 1) ^ (sector >> 2)) & 1 == 0;
                let (xa, ya) = if swap { (v, u) } else { (u, v) };
                let x = if sector & 4 == 0 { -xa } else { xa };
                let y = if sector & 1 == 0 { -ya } else { ya };
                Self::new(x, y)
            }
        })
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// A stone on the board, either black or white.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stone {
    /// The black stone.
    Black = 0,
    /// The white stone.
    White = 1,
}

impl Stone {
    /// Creates a stone from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        Some(match n {
            0 => Self::Black,
            1 => Self::White,
            _ => return None,
        })
    }

    /// Returns the opposite stone.
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Black => Self::White,
            Self::White => Self::Black,
        }
    }
}

// Allows room for extension. Equals (2^7-11^2).
const MOVE_PLACE_OFFSET: u32 = 7;

// For both full and delta encoding.
const MOVE_PASS: u32 = 0;
const MOVE_WIN: u32 = 1;
const MOVE_DRAW: u32 = 2;
const MOVE_RESIGN: u32 = 3;

// For delta encoding.
const MOVE_PLACE_SINGLE: u32 = 4;

const DELTA_ORIGIN_BLOCK_SIZES: &[u8] = &[3, 2];
const SHAPE_BLOCK_SIZES: &[u8] = &[3, 3, 2];

/// A move made by one player or both players.
#[derive(Clone, Copy, Debug)]
pub enum Move {
    /// One or two stones placed on the board by the current player.
    Place(Point, Option<Point>),
    /// A pass made by the current player.
    Pass,
    /// A winning row claimed by any player.
    Win(Point, Direction),
    /// A draw agreed by both players.
    Draw,
    /// A resignation indicated by the player with the given stone.
    Resign(Stone),
}

impl Move {
    /// Tests if the move is an ending one.
    #[must_use]
    pub fn is_ending(self) -> bool {
        matches!(self, Self::Win(..) | Self::Draw | Self::Resign(_))
    }

    /// Encodes the move to a buffer.
    ///
    /// If `compact`, omits the pass after a 1-stone move.
    pub fn encode(self, buf: &mut Vec<u8>, compact: bool) {
        match self {
            Self::Place(p1, p2) => {
                for p in iter::once(p1).chain(p2) {
                    let x = p.index() + MOVE_PLACE_OFFSET;
                    buf.put_u32_varint(x);
                }
                if p2.is_none() && !compact {
                    buf.put_u8(MOVE_PASS as u8);
                }
            }
            Self::Pass => buf.put_u8(MOVE_PASS as u8),
            Self::Win(p, dir) => {
                buf.put_u8(MOVE_WIN as u8);
                p.encode(buf);
                buf.put_u8(dir as u8);
            }
            Self::Draw => buf.put_u8(MOVE_DRAW as u8),
            Self::Resign(stone) => {
                buf.put_u8(MOVE_RESIGN as u8);
                buf.put_u8(stone as u8);
            }
        }
    }

    /// Decodes a move from a buffer.
    ///
    /// If `first`, eagerly returns a 1-stone move.
    #[must_use]
    pub fn decode(buf: &mut &[u8], first: bool) -> Option<Self> {
        let x = buf.try_get_u32_varint().ok()?;
        if let Some(i) = x.checked_sub(MOVE_PLACE_OFFSET) {
            let p1 = Point::from_index(i);

            if first {
                return Some(Self::Place(p1, None));
            }

            let mut p2 = None;
            let x = buf.try_get_u32_varint().ok()?;
            if let Some(i) = x.checked_sub(MOVE_PLACE_OFFSET) {
                p2 = Some(Point::from_index(i));
            } else if x != MOVE_PASS {
                return None;
            }
            return Some(Self::Place(p1, p2));
        }

        Some(match x {
            MOVE_WIN => Self::Win(
                Point::decode(buf)?,
                Direction::from_u8(buf.try_get_u8().ok()?)?,
            ),
            MOVE_RESIGN => Self::Resign(Stone::from_u8(buf.try_get_u8().ok()?)?),
            MOVE_PASS => Self::Pass,
            MOVE_DRAW => Self::Draw,
            _ => return None,
        })
    }

    fn encode_delta(self, writer: &mut BitWriter<'_>, origin: &mut Point) {
        if let Self::Place(p1, p2) = self {
            if let Some(p2) = p2 {
                let shape = p1 - p2;
                assert_ne!(shape, Point::ZERO);
                shape.encode_d4(SHAPE_BLOCK_SIZES, writer, true);
            } else {
                writer.write(0, SHAPE_BLOCK_SIZES[0]);
                writer.write(MOVE_PLACE_SINGLE as u8, 4);
            }

            let new_origin = p2.map_or(p1, |p2| p1.midpoint_floor(p2));
            let delta_origin = new_origin - *origin;
            *origin = new_origin;

            delta_origin.encode_d4(DELTA_ORIGIN_BLOCK_SIZES, writer, false);
            return;
        }

        writer.write(0, SHAPE_BLOCK_SIZES[0]);

        match self {
            Self::Place(..) => unreachable!(),
            Self::Pass => {
                writer.write(MOVE_PASS as u8, 4);
            }
            Self::Win(p, dir) => {
                writer.write(MOVE_WIN as u8, 4);

                let delta = p - *origin;
                let n = delta.new_index().unwrap();
                writer.write_u32_varint(n, 4);

                writer.write(dir as u8, 4);
            }
            Self::Draw => {
                writer.write(MOVE_DRAW as u8, 4);
            }
            Self::Resign(stone) => {
                writer.write(MOVE_RESIGN as u8, 4);
                writer.write(stone as u8, 4);
            }
        }
    }

    fn decode_delta(
        reader: &mut BitReader<'_, '_>,
        origin: &mut Point,
        first: bool,
    ) -> Option<Self> {
        let x = reader.read_u32_varint_with_sizes(SHAPE_BLOCK_SIZES)?;
        if x == 0 {
            if first && !reader.has_remaining() {
                return Some(Self::Place(Point::ZERO, None));
            }

            let kind = reader.read_u32_varint(4)?;
            Some(match kind {
                MOVE_PASS => Self::Pass,
                MOVE_WIN => {
                    let n = reader.read_u32_varint(4)?;
                    let delta = Point::from_new_index(n)?;

                    let pos = Point::new(
                        origin.x.checked_add(delta.x)?,
                        origin.y.checked_add(delta.y)?,
                    );
                    Self::Win(pos, Direction::from_u8(reader.read(4)?)?)
                }
                MOVE_DRAW => Self::Draw,
                MOVE_RESIGN => Self::Resign(Stone::from_u8(reader.read(4)?)?),
                MOVE_PLACE_SINGLE => {
                    let orbit = reader.read_u32_varint_with_sizes(DELTA_ORIGIN_BLOCK_SIZES)?;
                    let delta_origin = Point::decode_d4(orbit, reader, false)?;

                    origin.x = origin.x.checked_add(delta_origin.x)?;
                    origin.y = origin.y.checked_add(delta_origin.y)?;

                    Self::Place(*origin, None)
                }
                _ => return None,
            })
        } else {
            let shape = Point::decode_d4(x, reader, true)?;

            let orbit = reader.read_u32_varint_with_sizes(DELTA_ORIGIN_BLOCK_SIZES)?;
            let delta_origin = Point::decode_d4(orbit, reader, false)?;

            origin.x = origin.x.checked_add(delta_origin.x)?;
            origin.y = origin.y.checked_add(delta_origin.y)?;

            let dx = (shape.x >> 1) + (shape.x & 1);
            let dy = (shape.y >> 1) + (shape.y & 1);

            let x1 = origin.x.checked_add(dx)?;
            let y1 = origin.y.checked_add(dy)?;

            let x2 = x1.checked_sub(shape.x)?;
            let y2 = y1.checked_sub(shape.y)?;

            let p1 = Point::new(x1, y1);
            let p2 = Point::new(x2, y2);
            Some(Self::Place(p1, Some(p2)))
        }
    }
}

impl PartialEq for Move {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Place(p1, None), Self::Place(p2, None)) => p1 == p2,
            (Self::Place(p11, Some(p21)), Self::Place(p12, Some(p22))) => {
                (p11, p21) == (p12, p22) || (p11, p21) == (p22, p12)
            }
            (Self::Pass, Self::Pass) => true,
            (Self::Win(p1, d1), Self::Win(p2, d2)) => (p1, d1) == (p2, d2),
            (Self::Draw, Self::Draw) => true,
            (Self::Resign(s1), Self::Resign(s2)) => s1 == s2,
            _ => false,
        }
    }
}

impl Eq for Move {}

/// Returns the stone to play at the given move index.
#[must_use]
pub fn turn_at(index: usize) -> Stone {
    if index.is_multiple_of(2) {
        Stone::Black
    } else {
        Stone::White
    }
}

/// Scheme to encode a game record with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordEncodingScheme {
    /// Whether to include all moves prefixed with the current move index.
    pub all: bool,
    /// Whether to enable delta encoding.
    pub delta: bool,
}

impl RecordEncodingScheme {
    /// Returns the default scheme for encoding all moves.
    #[must_use]
    pub fn all() -> Self {
        Self {
            all: true,
            delta: true,
        }
    }

    /// Returns the default scheme for encoding only the past moves.
    #[must_use]
    pub fn past() -> Self {
        Self {
            all: false,
            delta: true,
        }
    }

    /// Encodes the scheme to a `u8`.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self.all as u8 | (self.delta as u8) << 1
    }

    /// Creates a `RecordEncodeScheme` from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        if n > 3 {
            return None;
        }

        let all = n & 1 != 0;
        let delta = n & 2 != 0;

        Some(Self { all, delta })
    }
}

/// A Connect6 game record.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Record {
    map: HashMap<Point, Stone>,
    moves: Vec<Move>,
    index: usize,
}

impl Record {
    /// Creates an empty record.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            moves: vec![],
            index: 0,
        }
    }

    /// Clears the record.
    pub fn clear(&mut self) {
        self.map.clear();
        self.moves.clear();
        self.index = 0;
    }

    /// Returns a slice of all moves, in the past or in the future.
    #[must_use]
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// Returns the current move index.
    #[must_use]
    pub fn move_index(&self) -> usize {
        self.index
    }

    /// Returns the previous move (if any).
    #[must_use]
    pub fn prev_move(&self) -> Option<Move> {
        self.index.checked_sub(1).map(|i| self.moves[i])
    }

    /// Returns the next move (if any).
    #[must_use]
    pub fn next_move(&self) -> Option<Move> {
        self.moves.get(self.index).copied()
    }

    /// Tests if there is any move in the past.
    #[must_use]
    pub fn has_past(&self) -> bool {
        self.index > 0
    }

    /// Tests if there is any move in the future.
    #[must_use]
    pub fn has_future(&self) -> bool {
        self.index < self.moves.len()
    }

    /// Tests if the game is ended.
    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.prev_move().is_some_and(Move::is_ending)
    }

    /// Returns the maximum number of stones to play in the current turn.
    #[must_use]
    pub fn max_stones_to_play(&self) -> usize {
        if !self.has_past() {
            1
        } else if !self.is_ended() {
            2
        } else {
            0
        }
    }

    /// Returns the current stone to play, without checking if the game is ended.
    fn turn_unchecked(&self) -> Stone {
        turn_at(self.index)
    }

    /// Returns the current stone to play, or `None` if the game is ended.
    #[must_use]
    pub fn turn(&self) -> Option<Stone> {
        (!self.is_ended()).then(|| self.turn_unchecked())
    }

    /// Returns the stone at the given position (if any).
    #[must_use]
    pub fn stone_at(&self, p: Point) -> Option<Stone> {
        self.map.get(&p).copied()
    }

    /// Makes a move, clearing moves in the future.
    ///
    /// Returns whether the move succeeded.
    pub fn make_move(&mut self, mov: Move) -> bool {
        if self.is_ended() {
            return false;
        }
        if self.index as u32 == u32::MAX {
            return false;
        }

        if let Move::Place(p1, p2) = mov {
            if self.index == 0 && p2.is_some() {
                return false;
            }
            if p2 == Some(p1) {
                return false;
            }

            for p in iter::once(p1).chain(p2) {
                // Avoid overflow for delta and varint encoding
                if p.x.unsigned_abs().max(p.y.unsigned_abs()) > 0x3fff {
                    return false;
                }
                if self.map.contains_key(&p) {
                    return false;
                }
            }

            let stone = self.turn_unchecked();
            for p in iter::once(p1).chain(p2) {
                self.map.insert(p, stone);
            }
        } else if let Move::Win(p, dir) = mov
            && self.test_winning_row(p, dir).is_none()
        {
            return false;
        }

        self.moves.truncate(self.index);
        self.moves.push(mov);
        self.index += 1;
        true
    }

    /// Undoes the previous move (if any).
    pub fn undo_move(&mut self) -> Option<Move> {
        let prev = self.prev_move()?;
        if let Move::Place(p1, p2) = prev {
            for p in iter::once(p1).chain(p2) {
                self.map.remove(&p);
            }
        }
        self.index -= 1;
        Some(prev)
    }

    /// Redoes the next move (if any).
    pub fn redo_move(&mut self) -> Option<Move> {
        let next = self.next_move()?;
        if let Move::Place(p1, p2) = next {
            let stone = self.turn_unchecked();
            for p in iter::once(p1).chain(p2) {
                self.map.insert(p, stone);
            }
        }
        self.index += 1;
        Some(next)
    }

    /// Jumps to the given move index by undoing or redoing moves.
    pub fn jump(&mut self, index: usize) -> bool {
        if index > self.moves.len() {
            return false;
        }
        if self.index > index {
            for _ in 0..self.index - index {
                self.undo_move();
            }
        } else {
            for _ in 0..index - self.index {
                self.redo_move();
            }
        }
        true
    }

    /// Returns an iterator of adjacent positions occupied by `stone`
    /// in the direction `dir`, starting from `p` (exclusive).
    fn scan(&self, p: Point, dir: Direction, stone: Stone) -> impl Iterator<Item = Point> {
        p.adjacent_iter(dir)
            .take_while(move |&p| self.stone_at(p) == Some(stone))
    }

    /// Searches in all directions for a winning row passing through `p`.
    ///
    /// If a winning row is found, returns one of its endpoints
    /// and a direction pointing to the other endpoint.
    #[must_use]
    pub fn find_winning_row(&self, p: Point) -> Option<(Point, Direction)> {
        let stone = self.stone_at(p)?;
        for (dir_fwd, dir_bwd) in Direction::OPPOSITE_PAIRS {
            let scan_fwd = self.scan(p, dir_fwd, stone).map(|p| (p, dir_bwd));
            let scan_bwd = self.scan(p, dir_bwd, stone).map(|p| (p, dir_fwd));

            if let Some(res) = scan_fwd.chain(scan_bwd).nth(4) {
                return Some(res);
            }
        }
        None
    }

    /// Tests if the given winning row is valid, returning the other endpoint if so.
    #[must_use]
    pub fn test_winning_row(&self, p: Point, dir: Direction) -> Option<Point> {
        self.scan(p, dir, self.stone_at(p)?).nth(4)
    }

    /// Places `stone` at each of `positions` temporarily, calls `f`
    /// and returns the result after undoing the placements.
    ///
    /// # Panics
    ///
    /// Panics if any of `positions` is occupied.
    pub fn with_temp_placements<T, F>(&mut self, stone: Stone, positions: &[Point], f: F) -> T
    where
        F: FnOnce(&Self) -> T,
    {
        for &p in positions {
            assert!(self.map.insert(p, stone).is_none());
        }
        let res = f(self);
        for p in positions {
            self.map.remove(p);
        }
        res
    }

    /// Encodes the record to a buffer.
    pub fn encode(&self, buf: &mut Vec<u8>, scheme: RecordEncodingScheme) {
        if scheme.delta {
            let mut writer = BitWriter::new(buf);
            writer.write(scheme.as_u8(), 4);

            let moves = if scheme.all {
                writer.write_u32_varint(self.index as u32, 4);
                &self.moves
            } else {
                &self.moves[..self.index]
            };

            if let [Move::Place(Point::ZERO, None)] = moves {
                writer.write(0, 3);
                return;
            }

            let mut origin = Point::ZERO;
            for (i, mov) in moves.iter().enumerate() {
                if i == 0
                    && let Move::Place(Point::ZERO, None) = mov
                    && let Some(Move::Place(_, Some(_))) = moves.get(1)
                {
                    continue;
                }
                mov.encode_delta(&mut writer, &mut origin);
            }
        } else {
            buf.put_u8(scheme.as_u8());

            let end = if scheme.all {
                buf.put_u32_varint(self.index as u32);
                self.moves.len()
            } else {
                self.index
            };

            for i in 0..end {
                self.moves[i].encode(buf, i == 0);
            }
        }
    }

    /// Encodes the record to a new buffer.
    #[must_use]
    pub fn encode_to_vec(&self, scheme: RecordEncodingScheme) -> Vec<u8> {
        let mut buf = vec![];
        self.encode(&mut buf, scheme);
        buf
    }

    /// Decodes a record from a buffer.
    #[must_use]
    pub fn decode(buf: &mut &[u8]) -> Option<Self> {
        if buf.is_empty() {
            return None;
        }

        let mut reader = BitReader::new(buf);
        let scheme = RecordEncodingScheme::from_u8(reader.read(4)?)?;

        if scheme.delta {
            let index = if scheme.all {
                Some(reader.read_u32_varint(4)?)
            } else {
                None
            };

            let mut record = Self::new();
            let mut origin = Point::ZERO;

            while reader.has_remaining() {
                let first = !record.has_past();
                let mov = Move::decode_delta(&mut reader, &mut origin, first)?;

                if first
                    && let Move::Place(_, Some(_)) = mov
                    && !record.make_move(Move::Place(Point::ZERO, None))
                {
                    return None;
                }

                if !record.make_move(mov) {
                    return None;
                }
            }

            if let Some(index) = index
                && !record.jump(index as usize)
            {
                return None;
            }
            Some(record)
        } else {
            let index = if scheme.all {
                Some(buf.try_get_u32_varint().ok()?)
            } else {
                None
            };

            let mut record = Self::new();

            while buf.has_remaining() {
                let mov = Move::decode(buf, !record.has_past())?;
                if !record.make_move(mov) {
                    return None;
                }
            }

            if let Some(index) = index
                && !record.jump(index as usize)
            {
                return None;
            }
            Some(record)
        }
    }
}

/// Players.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Player {
    /// One who starts the game.
    Host = 0,
    /// The other who joins the game.
    Guest = 1,
}

impl Player {
    /// Creates a player from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        Some(match n {
            0 => Self::Host,
            1 => Self::Guest,
            _ => return None,
        })
    }

    /// Returns the opposite player.
    #[must_use]
    pub fn opposite(self) -> Self {
        match self {
            Self::Host => Self::Guest,
            Self::Guest => Self::Host,
        }
    }
}

/// A struct to store data for both players.
#[derive(Debug, Default)]
pub struct PlayerSlots<T> {
    slots: [T; 2],
}

impl<T> PlayerSlots<T> {
    /// Fills both slots with the value.
    pub fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        self.slots.fill(value);
    }
}

impl<T> Index<Player> for PlayerSlots<T> {
    type Output = T;

    fn index(&self, player: Player) -> &T {
        &self.slots[player as usize]
    }
}

impl<T> IndexMut<Player> for PlayerSlots<T> {
    fn index_mut(&mut self, player: Player) -> &mut T {
        &mut self.slots[player as usize]
    }
}
