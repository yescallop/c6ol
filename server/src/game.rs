//! Connect6 game logic, record, and serialization.

use bytes::{Buf, BufMut};
use bytes_varint::{try_get_fixed::TryGetFixedSupport, VarIntSupport, VarIntSupportMut};
use std::{collections::HashMap, iter};

/// An axis on the board.
#[derive(Clone, Copy)]
pub enum Axis {
    /// The horizontal axis, with a unit vector of `(1, 0)`.
    Horizontal,
    /// The diagonal axis, with a unit vector of `(1, 1)`.
    Diagonal,
    /// The vertical axis, with a unit vector of `(0, 1)`.
    Vertical,
    /// The anti-diagonal axis, with a unit vector of `(1, -1)`.
    AntiDiagonal,
}

impl Axis {
    /// All axes on the board.
    pub const VALUES: [Self; 4] = [
        Self::Horizontal,
        Self::Diagonal,
        Self::Vertical,
        Self::AntiDiagonal,
    ];

    /// Returns the unit vector in the forward direction of the axis.
    pub fn unit_vector(self) -> (i16, i16) {
        [(1, 0), (1, 1), (0, 1), (1, -1)][self as usize]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    /// The horizontal coordinate.
    pub x: i16,
    /// The vertical coordinate.
    pub y: i16,
}

impl Point {
    /// Creates a point with the given coordinates.
    pub fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }

    /// Maps the point to a natural number.
    pub fn index(self) -> u32 {
        elegant_pair(zigzag_encode(self.x), zigzag_encode(self.y))
    }

    /// Maps a natural number to a point (undoes `index`).
    pub fn from_index(i: u32) -> Self {
        let (x, y) = elegant_unpair(i);
        Self::new(zigzag_decode(x), zigzag_decode(y))
    }

    /// Returns the adjacent point in the given direction,
    /// or `None` if overflow occurred.
    pub fn adjacent(self, axis: Axis, forward: bool) -> Option<Self> {
        let (dx, dy) = axis.unit_vector();
        Some(if forward {
            Self::new(self.x.checked_add(dx)?, self.y.checked_add(dy)?)
        } else {
            Self::new(self.x.checked_sub(dx)?, self.y.checked_sub(dy)?)
        })
    }

    /// Deserializes a point from a buffer.
    pub fn deserialize(buf: &mut &[u8]) -> Option<Self> {
        buf.try_get_u32_varint().ok().map(Self::from_index)
    }
}

/// A contiguous row of stones on the board.
#[derive(Clone, Copy)]
pub struct Row {
    /// The starting position of the row.
    pub start: Point,
    /// The ending position of the row.
    pub end: Point,
}

/// A stone on the board, either black or white.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stone {
    /// The black stone.
    Black = 1,
    /// The white stone.
    White = 2,
}

impl Stone {
    /// Creates a stone from a `u8`.
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Black),
            2 => Some(Self::White),
            _ => None,
        }
    }

    /// Returns the opposite stone.
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
#[derive(Debug, Clone, Copy)]
pub enum Move {
    /// One or two stones placed on the board by the current player.
    Stone(Point, Option<Point>),
    /// A pass made by the current player.
    Pass,
    /// A winning row at the given position claimed by any player.
    Win(Point),
    /// A draw agreed by both players.
    Draw,
    /// A resignation indicated by the player with the given stone.
    Resign(Stone),
}

impl Move {
    /// Tests if the move is an ending move.
    pub fn is_ending(self) -> bool {
        matches!(self, Self::Win(_) | Self::Draw | Self::Resign(_))
    }

    /// Serializes the move to a buffer.
    ///
    /// If `compact`, omits the pass after a 1-stone move.
    pub fn serialize(self, buf: &mut Vec<u8>, compact: bool) {
        match self {
            Self::Stone(fst, snd) => {
                for pos in iter::once(fst).chain(snd) {
                    let x = pos.index() + MOVE_STONE_OFFSET;
                    buf.put_u32_varint(x);
                }
                if snd.is_none() && !compact {
                    buf.put_u8(MOVE_PASS as u8);
                }
            }
            Self::Pass => buf.put_u8(MOVE_PASS as u8),
            Self::Win(pos) => {
                buf.put_u8(MOVE_WIN as u8);
                buf.put_u32_varint(pos.index());
            }
            Self::Draw => buf.put_u8(MOVE_DRAW as u8),
            Self::Resign(stone) => {
                buf.put_u8(MOVE_RESIGN as u8);
                buf.put_u8(stone as u8);
            }
        }
    }

    /// Deserializes a move from a buffer.
    ///
    /// If `first`, eagerly returns a 1-stone move.
    pub fn deserialize(buf: &mut &[u8], first: bool) -> Option<Self> {
        let x = buf.try_get_u32_varint().ok()?;
        if x >= MOVE_STONE_OFFSET {
            let fst = Point::from_index(x - MOVE_STONE_OFFSET);
            if first || !buf.has_remaining() {
                return Some(Self::Stone(fst, None));
            }

            let mut snd = None;
            let x = buf.try_get_u32_varint().ok()?;
            if x >= MOVE_STONE_OFFSET {
                snd = Some(Point::from_index(x - MOVE_STONE_OFFSET));
            } else if x != MOVE_PASS {
                return None;
            }
            return Some(Self::Stone(fst, snd));
        }

        match x {
            MOVE_WIN => {
                let pos = Point::deserialize(buf)?;
                Some(Self::Win(pos))
            }
            MOVE_RESIGN => {
                let stone = Stone::from_u8(buf.try_get_u8().ok()?)?;
                Some(Self::Resign(stone))
            }
            MOVE_PASS => Some(Self::Pass),
            MOVE_DRAW => Some(Self::Draw),
            _ => None,
        }
    }
}

/// A Connect6 game record on an infinite board.
#[derive(Clone, Default)]
pub struct Record {
    map: HashMap<Point, Stone>,
    moves: Vec<Move>,
    index: usize,
}

impl Record {
    /// Creates a new empty record.
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
    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// Returns the current move index.
    pub fn move_index(&self) -> usize {
        self.index
    }

    /// Returns the previous move (if any).
    pub fn prev_move(&self) -> Option<Move> {
        self.index.checked_sub(1).map(|i| self.moves[i])
    }

    /// Returns the next move (if any).
    pub fn next_move(&self) -> Option<Move> {
        self.moves.get(self.index).copied()
    }

    /// Tests if there is any move in the past.
    pub fn has_past(&self) -> bool {
        self.index > 0
    }

    /// Tests if there is any move in the future.
    pub fn has_future(&self) -> bool {
        self.index < self.moves.len()
    }

    /// Tests if the game is ended.
    pub fn is_ended(&self) -> bool {
        self.prev_move().is_some_and(Move::is_ending)
    }

    /// Returns the stone to play at the given move index.
    pub fn turn_at(index: usize) -> Stone {
        if index % 2 == 0 {
            Stone::Black
        } else {
            Stone::White
        }
    }

    /// Returns the current stone to play.
    pub fn turn(&self) -> Stone {
        Self::turn_at(self.index)
    }

    /// Returns the stone at the given position (if any).
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

        if let Move::Stone(fst, snd) = mov {
            if self.index == 0 && snd.is_some() {
                return false;
            }
            if self.map.contains_key(&fst) || snd.is_some_and(|pos| self.map.contains_key(&pos)) {
                return false;
            }

            let stone = self.turn();
            for pos in iter::once(fst).chain(snd) {
                self.map.insert(pos, stone);
            }
        } else if let Move::Win(pos) = mov {
            if self.find_win_row(pos).is_none() {
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
        self.index -= 1;

        if let Move::Stone(fst, snd) = prev {
            for pos in iter::once(fst).chain(snd) {
                self.map.remove(&pos);
            }
        }
        Some(prev)
    }

    /// Redoes the next move (if any).
    pub fn redo_move(&mut self) -> Option<Move> {
        let next = self.next_move()?;
        self.index += 1;

        let stone = self.turn();
        if let Move::Stone(fst, snd) = next {
            for pos in iter::once(fst).chain(snd) {
                self.map.insert(pos, stone);
            }
        }
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

    /// Scans the row through a position in both directions of an axis.
    pub fn scan_row(&self, pos: Point, axis: Axis) -> (Row, u16) {
        let (start, end);
        let Some(stone) = self.stone_at(pos) else {
            (start, end) = (pos, pos);
            return (Row { start, end }, 0);
        };

        let mut len = 1;
        let mut scan = |mut cur: Point, forward| {
            while let Some(next) = cur.adjacent(axis, forward) {
                if self.stone_at(next) == Some(stone) {
                    len += 1;
                    cur = next;
                } else {
                    break;
                }
            }
            cur
        };

        (start, end) = (scan(pos, false), scan(pos, true));
        (Row { start, end }, len)
    }

    /// Searches for a win row through a position.
    pub fn find_win_row(&self, pos: Point) -> Option<Row> {
        let _ = self.stone_at(pos)?;
        for axis in Axis::VALUES {
            let (row, len) = self.scan_row(pos, axis);
            if len >= 6 {
                return Some(row);
            }
        }
        None
    }

    /// Serializes the record to a buffer.
    ///
    /// If `all`, includes all moves prefixed with the current move index.
    pub fn serialize(&self, buf: &mut Vec<u8>, all: bool) {
        if all {
            buf.put_u64_varint(self.index as u64);
        }
        let end = if all { self.moves.len() } else { self.index };
        for i in 0..end {
            self.moves[i].serialize(buf, i == 0);
        }
    }

    /// Deserializes a record from a buffer.
    pub fn deserialize(buf: &mut &[u8], all: bool) -> Option<Self> {
        let mut rec = Self::new();
        let index = if all {
            Some(buf.try_get_usize_varint().ok()?)
        } else {
            None
        };

        while buf.has_remaining() {
            let mov = Move::deserialize(buf, !rec.has_past())?;
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
