//! Connect6 game logic, record, and serialization.

use bytes::{Buf, BufMut};
use bytes_varint::{VarIntSupport, VarIntSupportMut};
use std::{
    collections::HashMap,
    iter,
    ops::{Index, IndexMut},
};

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
    /// The origin.
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

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
    Black = 0,
    /// The white stone.
    White = 1,
}

impl Stone {
    /// Creates a stone from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Black),
            1 => Some(Self::White),
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
const MOVE_STONE_OFFSET: u64 = 7;

const MOVE_PASS: u64 = 0;
const MOVE_WIN: u64 = 1;
const MOVE_DRAW: u64 = 2;
const MOVE_RESIGN: u64 = 3;

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
                    let x = p.index() as u64 + MOVE_STONE_OFFSET;
                    buf.put_u64_varint(x);
                }
                if p2.is_none() && !compact {
                    buf.put_u8(MOVE_PASS as u8);
                }
            }
            Self::Pass => buf.put_u8(MOVE_PASS as u8),
            Self::Win(p, dir) => {
                buf.put_u8(MOVE_WIN as u8);
                buf.put_u32_varint(p.index());
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
        let x = buf.try_get_u64_varint().ok()?;
        if let Some(i) = x.checked_sub(MOVE_STONE_OFFSET) {
            let p1 = Point::from_index(i.try_into().ok()?);
            if first || !buf.has_remaining() {
                return Some(Self::Place(p1, None));
            }

            let mut p2 = None;
            let x = buf.try_get_u64_varint().ok()?;
            if let Some(i) = x.checked_sub(MOVE_STONE_OFFSET) {
                p2 = Some(Point::from_index(i.try_into().ok()?));
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
}

/// Methods to encode a game record with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordEncodeMethod {
    /// Include only past moves.
    Past = 0,
    /// Include all moves prefixed with the current move index.
    All = 1,
}

impl RecordEncodeMethod {
    /// Creates a `RecordEncodeMethod` from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        Some(match n {
            0 => Self::Past,
            1 => Self::All,
            _ => return None,
        })
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

        if let Move::Place(p1, p2) = mov {
            if self.index == 0 && p2.is_some() {
                return false;
            }
            if self.map.contains_key(&p1) || p2.is_some_and(|p| self.map.contains_key(&p)) {
                return false;
            }

            let stone = self.turn_unchecked();
            for p in iter::once(p1).chain(p2) {
                self.map.insert(p, stone);
            }
        } else if let Move::Win(p, dir) = mov {
            if self.test_winning_row(p, dir).is_none() {
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
    fn scan(&self, p: Point, dir: Direction, stone: Stone) -> impl Iterator<Item = Point> + '_ {
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
    pub fn encode(&self, buf: &mut Vec<u8>, method: RecordEncodeMethod) {
        buf.put_u8(method as u8);

        let end = match method {
            RecordEncodeMethod::Past => self.index,
            RecordEncodeMethod::All => {
                buf.put_u64_varint(self.index as u64);
                self.moves.len()
            }
        };

        for i in 0..end {
            self.moves[i].encode(buf, i == 0);
        }
    }

    /// Decodes a record from a buffer.
    #[must_use]
    pub fn decode(buf: &mut &[u8]) -> Option<Self> {
        let method = RecordEncodeMethod::from_u8(buf.try_get_u8().ok()?)?;

        let index = match method {
            RecordEncodeMethod::Past => None,
            RecordEncodeMethod::All => Some(buf.try_get_usize_varint().ok()?),
        };

        let mut record = Self::new();

        while buf.has_remaining() {
            let mov = Move::decode(buf, !record.has_past())?;
            if !record.make_move(mov) {
                return None;
            }
        }

        if let Some(index) = index {
            if !record.jump(index) {
                return None;
            }
        }
        Some(record)
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
        match n {
            0 => Some(Self::Host),
            1 => Some(Self::Guest),
            _ => None,
        }
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
