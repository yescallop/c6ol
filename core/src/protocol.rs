//! WebSocket protocol.

use crate::game::{Direction, Move, Point, Record, Stone};
use bytes::{Buf, BufMut};
use std::{iter, mem};
use strum::{EnumDiscriminants, FromRepr};

/// A passcode.
pub type Passcode = Box<[u8]>;
/// A game ID.
pub type GameId = [u8; 10];

/// A player's request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request {
    /// Ends the game in a draw.
    Draw = 0,
    /// Retracts the previous move.
    Retract = 1,
    /// Resets the game.
    Reset = 2,
}

impl Request {
    /// List of all available requests.
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
#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(FromRepr), name(ClientMessageKind), repr(u8), vis(pub(self)))]
pub enum ClientMessage {
    /// When sent upon connection, requests to start a new game.
    /// When sent after `Join`, requests to authenticate.
    Start(Passcode),
    /// When sent upon connection, requests to join an existing game.
    Join(GameId),
    /// Requests to place one or two stones.
    Place(Point, Option<Point>),
    /// Requests to pass.
    Pass,
    /// Claims a win.
    ClaimWin(Point, Direction),
    /// Resigns the game.
    Resign,
    /// Makes a request.
    Request(Request),
}

impl ClientMessage {
    /// Encodes the client message to a new buffer.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut buf = vec![ClientMessageKind::from(&self) as u8];
        match self {
            Self::Start(passcode) => buf.put_slice(&passcode),
            Self::Join(game_id) => buf.put_slice(&game_id),
            Self::Place(p1, p2) => {
                for p in iter::once(p1).chain(p2) {
                    p.encode(&mut buf);
                }
            }
            Self::Pass => {}
            Self::ClaimWin(p, dir) => {
                p.encode(&mut buf);
                buf.put_u8(dir as u8);
            }
            Self::Resign => {}
            Self::Request(req) => buf.put_u8(req as u8),
        }
        buf
    }

    /// Decodes a client message from a buffer.
    #[must_use]
    pub fn decode(mut buf: &[u8]) -> Option<Self> {
        use ClientMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Start => Self::Start(Box::from(mem::take(&mut buf))),
            Kind::Join => Self::Join(mem::take(&mut buf).try_into().ok()?),
            Kind::Place => {
                let p1 = Point::decode(&mut buf)?;
                let p2 = if buf.has_remaining() {
                    Some(Point::decode(&mut buf)?)
                } else {
                    None
                };
                Self::Place(p1, p2)
            }
            Kind::Pass => Self::Pass,
            Kind::ClaimWin => Self::ClaimWin(
                Point::decode(&mut buf)?,
                Direction::from_u8(buf.try_get_u8().ok()?)?,
            ),
            Kind::Resign => Self::Resign,
            Kind::Request => Self::Request(Request::from_u8(buf.try_get_u8().ok()?)?),
        };
        (!buf.has_remaining()).then_some(msg)
    }
}

/// A server message.
#[derive(Clone, EnumDiscriminants)]
#[strum_discriminants(derive(FromRepr), name(ServerMessageKind), repr(u8), vis(pub(self)))]
pub enum ServerMessage {
    /// The user is authenticated.
    /// Sent before `Record` with the game ID if a new game is started.
    Started(Stone, Option<GameId>),
    /// The entire record is updated.
    Record(Box<Record>),
    /// A move was made.
    Move(Move),
    /// The previous move was retracted.
    Retract,
    /// A player made a request.
    Request(Stone, Request),
}

impl ServerMessage {
    /// Encodes the server message to a new buffer.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut buf = vec![ServerMessageKind::from(&self) as u8];
        match self {
            Self::Started(stone, game_id) => {
                buf.put_u8(stone as u8);
                if let Some(id) = game_id {
                    buf.put_slice(&id);
                }
            }
            Self::Record(record) => record.encode(&mut buf, false),
            Self::Move(mov) => mov.encode(&mut buf, true),
            Self::Retract => {}
            Self::Request(stone, request) => {
                buf.put_u8(stone as u8);
                buf.put_u8(request as u8);
            }
        }
        buf
    }

    /// Decodes a server message from a buffer.
    #[must_use]
    pub fn decode(mut buf: &[u8]) -> Option<Self> {
        use ServerMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Started => {
                let stone = Stone::from_u8(buf.try_get_u8().ok()?)?;
                let game_id = if buf.has_remaining() {
                    Some(mem::take(&mut buf).try_into().ok()?)
                } else {
                    None
                };
                Self::Started(stone, game_id)
            }
            Kind::Record => Self::Record(Box::new(Record::decode(&mut buf, false)?)),
            Kind::Move => Self::Move(Move::decode(&mut buf, false)?),
            Kind::Retract => Self::Retract,
            Kind::Request => Self::Request(
                Stone::from_u8(buf.try_get_u8().ok()?)?,
                Request::from_u8(buf.try_get_u8().ok()?)?,
            ),
        };
        (!buf.has_remaining()).then_some(msg)
    }
}
