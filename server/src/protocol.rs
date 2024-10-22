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
    /// Requests a draw.
    RequestDraw = 10,
    /// Requests to retract the previous move.
    RequestRetract = 11,
}

impl ClientMessage {
    /// Deserializes a client message from a buffer.
    pub fn deserialize(mut buf: &[u8]) -> Option<Self> {
        use ClientMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Start => Self::Start(Box::from(mem::take(&mut buf))),
            Kind::Join => Self::Join(mem::take(&mut buf).try_into().ok()?),
            Kind::Place => {
                let fst = Point::deserialize(&mut buf)?;
                let snd = if buf.has_remaining() {
                    Some(Point::deserialize(&mut buf)?)
                } else {
                    None
                };
                Self::Place(fst, snd)
            }
            Kind::Pass => Self::Pass,
            Kind::ClaimWin => Self::ClaimWin(Point::deserialize(&mut buf)?),
            Kind::Resign => Self::Resign,
            Kind::RequestDraw => Self::RequestDraw,
            Kind::RequestRetract => Self::RequestRetract,
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
    /// A player requested a draw.
    RequestDraw(Stone) = 10,
    /// A player requested to retract the previous move.
    RequestRetract(Stone) = 11,
}

impl ServerMessage {
    /// Serializes the server message to a new buffer.
    pub fn serialize(self) -> Vec<u8> {
        let mut buf = vec![ServerMessageKind::from(&self) as u8];
        match self {
            Self::Started { stone, game_id } => {
                buf.put_u8(stone as u8);
                if let Some(id) = game_id {
                    buf.put_slice(&id);
                }
            }
            Self::Record(rec) => rec.serialize(&mut buf, false),
            Self::Move(mov) => mov.serialize(&mut buf, true),
            Self::Retract => {}
            Self::RequestDraw(stone) | Self::RequestRetract(stone) => buf.put_u8(stone as u8),
        }
        buf
    }
}
