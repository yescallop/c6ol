//! WebSocket protocol.

use crate::game::{Move, Point, Record, Stone};
use bytes::{Buf, BufMut};
use bytes_varint::try_get_fixed::TryGetFixedSupport;
use std::mem;
use strum::{EnumDiscriminants, FromRepr};

const GAME_ID_SIZE: usize = 10;

/// A passcode.
pub type Passcode = Box<[u8]>;
/// A game ID.
pub type GameId = [u8; GAME_ID_SIZE];

/// A player's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Ends the game in a draw.
    Draw = 0,
    /// Retracts the previous move.
    Retract = 1,
    /// Resets the game.
    Reset = 2,
}

impl Request {
    /// All requests available.
    pub const VALUES: [Self; 3] = [Self::Draw, Self::Retract, Self::Reset];

    /// Creates a request from a `u8`.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Draw),
            1 => Some(Self::Retract),
            2 => Some(Self::Reset),
            _ => None,
        }
    }
}

/// A client message.
#[derive(Debug, Clone, EnumDiscriminants)]
#[repr(u8)]
#[strum_discriminants(derive(FromRepr), name(ClientMessageKind), vis(pub(self)))]
pub enum ClientMessage {
    /// When sent upon connection, requests to start a new game.
    /// When sent after `Join`, requests to authenticate.
    Start(Passcode) = 0,
    /// When sent upon connection, requests to join an existing game.
    Join(GameId) = 1,
    /// Requests to place one or two stones.
    Place(Point, Option<Point>) = 2,
    /// Requests to pass.
    Pass = 3,
    /// Claims a win.
    ClaimWin(Point) = 4,
    /// Resigns the game.
    Resign = 5,
    /// Makes a request.
    Request(Request) = 10,
}

impl ClientMessage {
    /// Decodes a client message from a buffer.
    #[must_use]
    pub fn decode(mut buf: &[u8]) -> Option<Self> {
        use ClientMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Start => Self::Start(Box::from(mem::take(&mut buf))),
            Kind::Join => Self::Join(mem::take(&mut buf).try_into().ok()?),
            Kind::Place => {
                let fst = Point::decode(&mut buf)?;
                let snd = if buf.has_remaining() {
                    Some(Point::decode(&mut buf)?)
                } else {
                    None
                };
                Self::Place(fst, snd)
            }
            Kind::Pass => Self::Pass,
            Kind::ClaimWin => Self::ClaimWin(Point::decode(&mut buf)?),
            Kind::Resign => Self::Resign,
            Kind::Request => Self::Request(Request::from_u8(buf.try_get_u8().ok()?)?),
        };
        (!buf.has_remaining()).then_some(msg)
    }
}

/// A server message.
#[derive(Clone, EnumDiscriminants)]
#[strum_discriminants(name(ServerMessageKind), vis(pub(self)))]
#[repr(u8)]
pub enum ServerMessage {
    /// The user is authenticated.
    /// Sent before `Record` if a new game is started.
    Started {
        /// The user's stone.
        stone: Stone,
        /// The game ID if a new game is started.
        game_id: Option<GameId>,
    } = 6,
    /// The entire record is updated.
    Record(Box<Record>) = 7,
    /// A move was made.
    Move(Move) = 8,
    /// The previous move was retracted.
    Retract = 9,
    /// A player made a request.
    Request(Request, Stone) = 10,
}

impl ServerMessage {
    /// Encodes the server message to a new buffer.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut buf = vec![ServerMessageKind::from(&self) as u8];
        match self {
            Self::Started { stone, game_id } => {
                buf.put_u8(stone as u8);
                if let Some(id) = game_id {
                    buf.put_slice(&id);
                }
            }
            Self::Record(rec) => rec.encode(&mut buf, false),
            Self::Move(mov) => mov.encode(&mut buf, true),
            Self::Retract => {}
            Self::Request(request, stone) => {
                buf.put_u8(request as u8);
                buf.put_u8(stone as u8);
            }
        }
        buf
    }
}
