//! Connect6 game logic, record, and serialization.

use bytes::{Buf, BufMut};
use bytes_varint::{try_get_fixed::TryGetFixedSupport, VarIntSupport, VarIntSupportMut};
use itertools::{Either, Itertools};
use std::{collections::HashMap, iter};

/// A direction on the board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// North, with a unit vector of `(0, -1)`.
    North,
    /// Northeast, with a unit vector of `(-1, 1)`.
    Northeast,
    /// East, with a unit vector of `(1, 0)`.
    East,
    /// Southeast, with a unit vector of `(1, 1)`.
    Southeast,
    /// South, with a unit vector of `(0, 1)`.
    South,
    /// Southwest, with a unit vector of `(1, -1)`.
    Southwest,
    /// West, with a unit vector of `(-1, 0)`.
    West,
    /// Northwest, with a unit vector of `(-1, -1)`.
    Northwest,
}

impl Direction {
    /// Four pairs of opposite directions.
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

    /// Returns the unit vector in this direction.
    #[must_use]
    pub fn unit_vector(self) -> (i16, i16) {
        match self {
            Self::North => (0, -1),
            Self::Northeast => (-1, 1),
            Self::East => (1, 0),
            Self::Southeast => (1, 1),
            Self::South => (0, 1),
            Self::Southwest => (1, -1),
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
fn elegant_pair(x: u16, y: u16) -> u32 {
    let (x, y) = (x as u32, y as u32);
    if x < y {
        y * y + x
    } else {
        x * x + x + y
    }
}

/// Maps one natural number to two (undoes `elegant_pair`).
fn elegant_unpair(z: u32) -> (u16, u16) {
    let s = (z as f64).sqrt() as u32;
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
    /// The horizontal coordinate.
    pub x: i16,
    /// The vertical coordinate.
    pub y: i16,
}

impl Point {
    /// Creates a point with the given coordinates.
    #[must_use]
    pub fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }

    /// Maps the point to a natural number.
    #[must_use]
    pub fn index(self) -> u32 {
        elegant_pair(zigzag_encode(self.x), zigzag_encode(self.y))
    }

    /// Maps a natural number to a point (undoes `index`).
    #[must_use]
    pub fn from_index(i: u32) -> Self {
        let (x, y) = elegant_unpair(i);
        Self::new(zigzag_decode(x), zigzag_decode(y))
    }

    /// Returns the adjacent point in the given direction,
    /// or `None` if overflow occurred.
    #[must_use]
    pub fn adjacent(self, dir: Direction) -> Option<Self> {
        let (dx, dy) = dir.unit_vector();
        Some(Self::new(self.x.checked_add(dx)?, self.y.checked_add(dy)?))
    }

    /// Returns an iterator of adjacent points in the given direction.
    pub fn adjacent_iter(self, dir: Direction) -> impl Iterator<Item = Self> {
        let mut cur = self;
        let (dx, dy) = dir.unit_vector();

        iter::from_fn(move || {
            cur = Self::new(cur.x.checked_add(dx)?, cur.y.checked_add(dy)?);
            Some(cur)
        })
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

/// A stone on the board, either black or white.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stone {
    /// The black stone.
    Black = 1,
    /// The white stone.
    White = 2,
}

impl Stone {
    /// Creates a stone from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Black),
            2 => Some(Self::White),
            _ => None,
        }
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

/// Allows room for extension. Equals (2^7-11^2).
const MOVE_STONE_OFFSET: u32 = 7;

const MOVE_PASS: u32 = 0;
const MOVE_WIN: u32 = 1;
const MOVE_DRAW: u32 = 2;
const MOVE_RESIGN: u32 = 3;

/// A move made by one player or both players.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// Tests if the move is an ending move.
    #[must_use]
    pub fn is_ending(self) -> bool {
        matches!(self, Self::Win(_, _) | Self::Draw | Self::Resign(_))
    }

    /// Encodes the move to a buffer.
    ///
    /// If `compact`, omits the pass after a 1-stone move.
    pub fn encode(self, buf: &mut Vec<u8>, compact: bool) {
        match self {
            Self::Place(fst, snd) => {
                for pos in iter::once(fst).chain(snd) {
                    let x = pos.index() + MOVE_STONE_OFFSET;
                    buf.put_u32_varint(x);
                }
                if snd.is_none() && !compact {
                    buf.put_u8(MOVE_PASS as u8);
                }
            }
            Self::Pass => buf.put_u8(MOVE_PASS as u8),
            Self::Win(pos, dir) => {
                buf.put_u8(MOVE_WIN as u8);
                buf.put_u32_varint(pos.index());
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
        if x >= MOVE_STONE_OFFSET {
            let fst = Point::from_index(x - MOVE_STONE_OFFSET);
            if first || !buf.has_remaining() {
                return Some(Self::Place(fst, None));
            }

            let mut snd = None;
            let x = buf.try_get_u32_varint().ok()?;
            if x >= MOVE_STONE_OFFSET {
                snd = Some(Point::from_index(x - MOVE_STONE_OFFSET));
            } else if x != MOVE_PASS {
                return None;
            }
            return Some(Self::Place(fst, snd));
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
}

/// A Connect6 game record on an infinite board.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct Record {
    map: HashMap<Point, Stone>,
    moves: Vec<Move>,
    index: usize,
}

impl Record {
    /// Creates a new empty record.
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

    /// Returns the stone to play at the given move index.
    #[must_use]
    pub fn turn_at(index: usize) -> Stone {
        if index % 2 == 0 {
            Stone::Black
        } else {
            Stone::White
        }
    }

    /// Returns the current stone to play, without checking if the game is ended.
    fn turn_unchecked(&self) -> Stone {
        Self::turn_at(self.index)
    }

    /// Returns the current stone to play, or `None` if the game is ended.
    #[must_use]
    pub fn turn(&self) -> Option<Stone> {
        (!self.is_ended()).then(|| self.turn_unchecked())
    }

    /// Returns the stone at the given position (if any).
    #[must_use]
    pub fn stone_at(&self, pos: Point) -> Option<Stone> {
        self.map.get(&pos).copied()
    }

    /// Makes a move, clearing moves in the future.
    ///
    /// Returns whether the move succeeded.
    pub fn make_move(&mut self, mov: Move) -> bool {
        if self.is_ended() {
            return false;
        }

        if let Move::Place(fst, snd) = mov {
            if self.index == 0 && snd.is_some() {
                return false;
            }
            if self.map.contains_key(&fst) || snd.is_some_and(|pos| self.map.contains_key(&pos)) {
                return false;
            }

            let stone = self.turn_unchecked();
            for pos in iter::once(fst).chain(snd) {
                self.map.insert(pos, stone);
            }
        } else if let Move::Win(pos, dir) = mov {
            if self.test_winning_row(pos, dir).is_none() {
                return false;
            }
        }

        self.moves.truncate(self.index);
        self.moves.push(mov);
        self.index += 1;
        true
    }

    /// Undoes the previous move (if any).
    pub fn undo_move(&mut self) -> Option<Move> {
        let prev = self.prev_move()?;
        if let Move::Place(fst, snd) = prev {
            for pos in iter::once(fst).chain(snd) {
                self.map.remove(&pos);
            }
        }
        self.index -= 1;
        Some(prev)
    }

    /// Redoes the next move (if any).
    pub fn redo_move(&mut self) -> Option<Move> {
        let next = self.next_move()?;
        if let Move::Place(fst, snd) = next {
            let stone = self.turn_unchecked();
            for pos in iter::once(fst).chain(snd) {
                self.map.insert(pos, stone);
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
    /// in the direction `dir`, starting from `pos` (exclusive).
    fn scan_row(
        &self,
        pos: Point,
        dir: Direction,
        stone: Stone,
    ) -> impl Iterator<Item = Point> + use<'_> {
        pos.adjacent_iter(dir)
            .take_while(move |&pos| self.stone_at(pos) == Some(stone))
    }

    /// Searches in all directions for a winning row through `pos`.
    ///
    /// When a winning row is found, returns one of its endpoints
    /// and a direction pointing to the other endpoint.
    #[must_use]
    pub fn find_winning_row(&self, pos: Point) -> Option<(Point, Direction)> {
        let stone = self.stone_at(pos)?;
        for (dir_fwd, dir_bwd) in Direction::OPPOSITE_PAIRS {
            let row_fwd = self.scan_row(pos, dir_fwd, stone).map(Either::Left);
            let row_bwd = self.scan_row(pos, dir_bwd, stone).map(Either::Right);

            if let Some(pos) = row_fwd.interleave(row_bwd).nth(4) {
                return Some(match pos {
                    Either::Left(pos) => (pos, dir_bwd),
                    Either::Right(pos) => (pos, dir_fwd),
                });
            }
        }
        None
    }

    /// Tests if the given winning row is valid, returning the other endpoint if so.
    #[must_use]
    pub fn test_winning_row(&self, pos: Point, dir: Direction) -> Option<Point> {
        self.scan_row(pos, dir, self.stone_at(pos)?).nth(4)
    }

    /// Encodes the record to a buffer.
    ///
    /// If `all`, includes all moves prefixed with the current move index.
    pub fn encode(&self, buf: &mut Vec<u8>, all: bool) {
        if all {
            buf.put_u64_varint(self.index as u64);
        }
        let end = if all { self.moves.len() } else { self.index };
        for i in 0..end {
            self.moves[i].encode(buf, i == 0);
        }
    }

    /// Decodes a record from a buffer.
    #[must_use]
    pub fn decode(buf: &mut &[u8], all: bool) -> Option<Self> {
        let mut rec = Self::new();
        let index = if all {
            Some(buf.try_get_usize_varint().ok()?)
        } else {
            None
        };

        while buf.has_remaining() {
            let mov = Move::decode(buf, !rec.has_past())?;
            if !rec.make_move(mov) {
                return None;
            }
        }

        if let Some(index) = index {
            if !rec.jump(index) {
                return None;
            }
        }
        Some(rec)
    }
}
