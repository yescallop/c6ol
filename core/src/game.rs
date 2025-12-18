//! Connect6 game logic, record, and serialization.

mod nibble;

#[cfg(test)]
mod tests;

use bytes::{Buf, BufMut};
use bytes_varint::{VarIntSupport, VarIntSupportMut};
use std::{
    collections::HashMap,
    iter,
    ops::{Add, AddAssign, Sub, SubAssign},
};

use nibble::{NibbleReader, NibbleWriter};

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
    /// Four canonical directions.
    pub const VALUES_CANONICAL: [Self; 4] =
        [Self::North, Self::Northeast, Self::East, Self::Southeast];

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

    /// Checks if this direction is canonical, with value less than 4.
    #[must_use]
    pub fn is_canonical(self) -> bool {
        (self as u8) < 4
    }

    /// Returns the opposite direction.
    #[must_use]
    pub fn opposite(self) -> Self {
        Self::from_u8(self as u8 ^ 4).unwrap()
    }

    /// Creates a direction from a unit vector.
    #[must_use]
    pub fn from_unit_vec(v: Point) -> Option<Self> {
        Some(match (v.x, v.y) {
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

    /// Returns an offset of `n` units in this direction.
    #[must_use]
    pub fn offset(self, n: i16) -> Point {
        let (x, y) = match self {
            Self::North => (0, -n),
            Self::Northeast => (n, -n),
            Self::East => (n, 0),
            Self::Southeast => (n, n),
            Self::South => (0, n),
            Self::Southwest => (-n, n),
            Self::West => (-n, 0),
            Self::Northwest => (-n, -n),
        };
        Point::new(x, y)
    }
}

fn zigzag_encode_16(n: i16) -> u16 {
    ((n << 1) ^ (n >> 15)) as u16
}

fn zigzag_decode_16(n: u16) -> i16 {
    ((n >> 1) ^ (n & 1).wrapping_neg()) as i16
}

fn zigzag_encode_32(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

fn zigzag_decode_32(n: u32) -> i32 {
    ((n >> 1) ^ (n & 1).wrapping_neg()) as i32
}

fn szudzik_pair(x: u16, y: u16) -> u32 {
    let (x, y) = (x as u32, y as u32);
    if x < y { y * y + x } else { x * x + x + y }
}

fn szudzik_unpair(z: u32) -> (u16, u16) {
    let s = z.isqrt();
    let t = z - s * s;
    if t < s {
        (t as u16, s as u16)
    } else {
        (s as u16, (t - s) as u16)
    }
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
        szudzik_pair(zigzag_encode_16(self.x), zigzag_encode_16(self.y))
    }

    /// Maps a natural number to a point (undoes `index`).
    #[must_use]
    pub fn from_index(i: u32) -> Self {
        let (x, y) = szudzik_unpair(i);
        Self::new(zigzag_decode_16(x), zigzag_decode_16(y))
    }

    /// Maps the point to a natural number, with each orbit under the action
    /// of the dihedral group D_4 mapped to a contiguous block of numbers.
    #[must_use]
    pub fn d4_index(self) -> u32 {
        let mut n = self.d4_centrosymmetric_index() as i32;
        if (self.x, self.y) < (0, 0) {
            n = -n;
        }
        zigzag_encode_32(n)
    }

    /// Maps a natural number to a point (undoes `d4_index`).
    #[must_use]
    pub fn from_d4_index(n: u32) -> Option<Self> {
        let n = zigzag_decode_32(n);
        let mut p = Self::from_d4_centrosymmetric_index(n.unsigned_abs())?;
        if n < 0 {
            p.x = -p.x;
            p.y = -p.y;
        }
        Some(p)
    }

    /// Maps the point to a natural number, with each orbit under the action
    /// of the dihedral group D_4 mapped to a contiguous block of numbers,
    /// and a set of centrosymmetric points mapped to a single number.
    #[must_use]
    pub fn d4_centrosymmetric_index(self) -> u32 {
        let (x, y) = (self.x as i32, self.y as i32);
        let u = x.unsigned_abs();
        let v = y.unsigned_abs();

        let s = u.max(v);
        if s == 0 {
            return 0;
        }

        let base = 2 * s * s - 2 * s + 1;
        let k = if u <= v { 2 * u } else { 2 * v + 1 };
        let offset = match k {
            0 | 1 => k, // (0, s), (s, 0)
            _ => 2 * k - 1 - ((x ^ y) < 0) as u32,
        };
        base + offset
    }

    /// Maps a natural number to a point, the lexicographically greater one in
    /// a set of centrosymmetric points (undoes `d4_centrosymmetric_index`).
    #[must_use]
    pub fn from_d4_centrosymmetric_index(n: u32) -> Option<Self> {
        const MAX_N: u32 = 2 * 0x8000 * 0x8000 - 2 * 0x8000;

        if n == 0 {
            return Some(Self::new(0, 0));
        }
        if n > MAX_N {
            return None;
        }

        let s = (2 * n - 1).isqrt().div_ceil(2);
        let base = 2 * s * s - 2 * s + 1;
        let offset = n - base;

        let s = s as i16;
        Some(match offset {
            0 => Self::new(0, s),
            1 => Self::new(s, 0),
            _ => {
                let k = offset / 2 + 1;
                let h = (k / 2) as i16;
                let (u, v) = if k & 1 == 0 { (h, s) } else { (s, h) };

                let y = if offset & 1 == 0 { -v } else { v };
                Self::new(u, y)
            }
        })
    }

    /// Returns an iterator of adjacent points in the given direction.
    pub fn adjacent_iter(self, dir: Direction) -> impl Iterator<Item = Self> {
        let mut cur = self;
        let unit_vec = dir.offset(1);

        iter::from_fn(move || {
            cur += unit_vec;
            Some(cur)
        })
    }

    /// Performs checked addition.
    #[must_use]
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        let x = self.x.checked_add(rhs.x)?;
        let y = self.y.checked_add(rhs.y)?;
        Some(Self::new(x, y))
    }

    /// Performs checked subtraction.
    #[must_use]
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        let x = self.x.checked_sub(rhs.x)?;
        let y = self.y.checked_sub(rhs.y)?;
        Some(Self::new(x, y))
    }

    /// Calculates the midpoint of two points, rounding towards negative infinity.
    #[must_use]
    pub fn midpoint_floor(self, rhs: Self) -> Self {
        let x = (self.x as i32 + rhs.x as i32) >> 1;
        let y = (self.y as i32 + rhs.y as i32) >> 1;
        Self::new(x as i16, y as i16)
    }

    /// Halves the coordinates, rounding towards positive infinity.
    #[must_use]
    pub fn half_ceil(self) -> Self {
        let x = (self.x as i32 + 1) >> 1;
        let y = (self.y as i32 + 1) >> 1;
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
}

impl Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Point {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Point {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
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

    fn encode_delta(self, writer: &mut NibbleWriter<'_>, origin: &mut Point) {
        if let Self::Place(p1, p2) = self {
            if let Some(p2) = p2 {
                let shape = p2 - p1;
                assert_ne!(shape, Point::ZERO);
                writer.write_u32_varint(shape.d4_centrosymmetric_index());
            } else {
                writer.write_u3(0);
                writer.write_u3(MOVE_PLACE_SINGLE as u8);
            }

            let new_origin = p2.map_or(p1, |p2| p1.midpoint_floor(p2));
            let delta_origin = new_origin - *origin;
            *origin = new_origin;

            writer.write_u32_varint(delta_origin.d4_index());
            return;
        }

        writer.write_u3(0);

        match self {
            Self::Place(..) => unreachable!(),
            Self::Pass => {
                writer.write_u3(MOVE_PASS as u8);
            }
            Self::Win(p, dir) => {
                writer.write_u3(MOVE_WIN as u8);

                let (third, dir) = if dir.is_canonical() {
                    (p + dir.offset(2), dir)
                } else {
                    (p + dir.offset(3), dir.opposite())
                };
                writer.write_u3(dir as u8);

                let delta = third - *origin;
                writer.write_u32_varint(delta.d4_index());
            }
            Self::Draw => {
                writer.write_u3(MOVE_DRAW as u8);
            }
            Self::Resign(stone) => {
                writer.write_u3(MOVE_RESIGN as u8);
                writer.write_u3(stone as u8);
            }
        }
    }

    fn decode_delta(
        reader: &mut NibbleReader<'_, '_>,
        origin: &mut Point,
        first: bool,
    ) -> Option<Self> {
        let x = reader.read_u32_varint()?;
        if x == 0 {
            if first && !reader.has_remaining() {
                return Some(Self::Place(Point::ZERO, None));
            }

            let kind = reader.read_u32_varint()?;
            Some(match kind {
                MOVE_PASS => Self::Pass,
                MOVE_WIN => {
                    let dir = Direction::from_u8(reader.read_u3()?)?;
                    if !dir.is_canonical() {
                        return None;
                    }

                    let n = reader.read_u32_varint()?;
                    let delta = Point::from_d4_index(n)?;

                    let third = origin.checked_add(delta)?;
                    let first = third.checked_add(dir.offset(-2))?;
                    Self::Win(first, dir)
                }
                MOVE_DRAW => Self::Draw,
                MOVE_RESIGN => Self::Resign(Stone::from_u8(reader.read_u3()?)?),
                MOVE_PLACE_SINGLE => {
                    let n = reader.read_u32_varint()?;
                    let delta_origin = Point::from_d4_index(n)?;

                    *origin = origin.checked_add(delta_origin)?;
                    Self::Place(*origin, None)
                }
                _ => return None,
            })
        } else {
            let shape = Point::from_d4_centrosymmetric_index(x)?;

            let n = reader.read_u32_varint()?;
            let delta_origin = Point::from_d4_index(n)?;

            *origin = origin.checked_add(delta_origin)?;

            let p2 = origin.checked_add(shape.half_ceil())?;
            let p1 = p2.checked_sub(shape)?;
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
            (Self::Win(p1, d1), Self::Win(p2, d2)) => {
                (p1, d1) == (p2, d2) || (p1 + d1.offset(5), d1.opposite()) == (p2, d2)
            }
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
    Stone::from_u8(index as u8 & 1).unwrap()
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
            delta: false,
        }
    }

    /// Returns the default scheme for encoding only the past moves.
    #[must_use]
    pub fn past() -> Self {
        Self {
            all: false,
            delta: false,
        }
    }

    /// Enables delta encoding.
    #[must_use]
    pub fn delta(mut self) -> Self {
        self.delta = true;
        self
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
        if self.index >= u32::MAX as usize {
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
        for dir_fwd in Direction::VALUES_CANONICAL {
            let dir_bwd = dir_fwd.opposite();

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
            let mut writer = NibbleWriter::new(buf);
            writer.write_u3(scheme.as_u8());

            let mut moves = if scheme.all {
                writer.write_u32_varint(self.index as u32);
                &self.moves
            } else {
                &self.moves[..self.index]
            };

            if let [Move::Place(Point::ZERO, None)] = moves {
                writer.write_u3(0);
                return;
            }

            if let [Move::Place(Point::ZERO, None), Move::Place(_, Some(_)), ..] = moves {
                moves = &moves[1..];
            }

            let mut origin = Point::ZERO;
            for mov in moves {
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

        let mut reader = NibbleReader::new(buf);
        let scheme = RecordEncodingScheme::from_u8(reader.read_u3()?)?;

        if scheme.delta {
            let index = if scheme.all {
                Some(reader.read_u32_varint()?)
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
