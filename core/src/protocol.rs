//! WebSocket protocol.

use crate::game::{Direction, Move, Player, Point, Record, RecordEncodingScheme, Stone};
use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};
use std::{fmt, iter, str};
use strum::{EnumDiscriminants, FromRepr};

/// A passcode.
pub type Passcode = Box<[u8]>;

/// A passcode hash.
pub type PasscodeHash = i64;

/// A game ID.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GameId(pub i64);

const BASE62_ALPHABET: &[u8] = b"0123456789\
    ABCDEFGHIJKLMNOPQRSTUVWXYZ\
    abcdefghijklmnopqrstuvwxyz";

const BASE62_LUT: &[i8] = &{
    let mut out = [-1; 256];
    let mut i = 0;
    while i < 62 {
        out[BASE62_ALPHABET[i] as usize] = i as i8;
        i += 1;
    }
    out
};

impl GameId {
    /// Decodes a Base62-encoded game ID.
    #[must_use]
    pub fn from_base62(buf: &[u8]) -> Option<Self> {
        if buf.len() != 11 {
            return None;
        }

        let mut n = 0u64;
        for &x in buf {
            let x = BASE62_LUT[x as usize];
            if x < 0 {
                return None;
            }
            n = n.checked_mul(62)?.checked_add(x as u64)?;
        }
        Some(Self(n as i64))
    }
}

impl fmt::Display for GameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut n = self.0 as u64;
        let mut buf = [b'0'; 11];
        let mut i = 11;
        while n > 0 {
            i -= 1;
            buf[i] = BASE62_ALPHABET[(n % 62) as usize];
            n /= 62;
        }
        f.write_str(str::from_utf8(&buf).unwrap())
    }
}

/// Trait for types that can be encoded to and decoded from a buffer.
pub trait Message: Sized {
    /// Encodes the message to a buffer.
    fn encode(self, buf: &mut Vec<u8>);

    /// Encodes the message to a new buffer.
    #[must_use]
    fn encode_to_vec(self) -> Vec<u8> {
        let mut buf = vec![];
        self.encode(&mut buf);
        buf
    }

    /// Decodes a message from a buffer.
    #[must_use]
    fn decode(buf: &mut &[u8]) -> Option<Self>;
}

/// Game options.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameOptions {
    /// Whether the stones are swapped.
    pub swapped: bool,
}

impl GameOptions {
    /// Returns a player's stone given the options.
    #[must_use]
    pub fn stone_of(self, player: Player) -> Stone {
        let orig_stone = match player {
            Player::Host => Stone::Black,
            Player::Guest => Stone::White,
        };
        if self.swapped {
            orig_stone.opposite()
        } else {
            orig_stone
        }
    }
}

impl Message for GameOptions {
    fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u8(self.swapped as u8);
    }

    fn decode(buf: &mut &[u8]) -> Option<Self> {
        Some(Self {
            swapped: match buf.try_get_u8().ok()? {
                0 => false,
                1 => true,
                _ => return None,
            },
        })
    }
}

/// A player's request.
#[derive(Clone, Copy, Debug, EnumDiscriminants, Eq, PartialEq)]
#[repr(u8)]
#[strum_discriminants(derive(FromRepr), name(RequestKind), vis(pub(self)))]
pub enum Request {
    /// Ends the game in a draw.
    Draw = 1,
    /// Retracts the previous move.
    Retract = 2,
    /// Resets the game.
    Reset(GameOptions) = 3,
}

impl Message for Request {
    fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u8(RequestKind::from(self) as u8);
        match self {
            Self::Draw | Self::Retract => {}
            Self::Reset(options) => options.encode(buf),
        }
    }

    fn decode(buf: &mut &[u8]) -> Option<Self> {
        use RequestKind as Kind;

        Some(match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Draw => Self::Draw,
            Kind::Retract => Self::Retract,
            Kind::Reset => Self::Reset(GameOptions::decode(buf)?),
        })
    }
}

/// A client message.
#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(FromRepr), name(ClientMessageKind), repr(u8), vis(pub(self)))]
pub enum ClientMessage {
    /// Requests to start a new game.
    Start(GameOptions),
    /// Requests to join an existing game.
    Join(GameId),
    /// Requests to authenticate.
    Authenticate(PasscodeHash),
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
    /// Accepts the opponent's request.
    AcceptRequest,
    /// Declines the opponent's request.
    DeclineRequest,
}

impl Message for ClientMessage {
    fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u8(ClientMessageKind::from(&self) as u8);
        match self {
            Self::Start(options) => options.encode(buf),
            Self::Join(game_id) => buf.put_i64(game_id.0),
            Self::Authenticate(hash) => buf.put_i64(hash),
            Self::Place(p1, p2) => {
                for p in iter::once(p1).chain(p2) {
                    p.encode(buf);
                }
            }
            Self::Pass => {}
            Self::ClaimWin(p, dir) => {
                p.encode(buf);
                buf.put_u8(dir as u8);
            }
            Self::Resign => {}
            Self::Request(req) => req.encode(buf),
            Self::AcceptRequest | Self::DeclineRequest => {}
        }
    }

    fn decode(buf: &mut &[u8]) -> Option<Self> {
        use ClientMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Start => Self::Start(GameOptions::decode(buf)?),
            Kind::Join => Self::Join(GameId(buf.try_get_i64().ok()?)),
            Kind::Authenticate => Self::Authenticate(buf.try_get_i64().ok()?),
            Kind::Place => {
                let p1 = Point::decode(buf)?;
                let p2 = if buf.has_remaining() {
                    Some(Point::decode(buf)?)
                } else {
                    None
                };
                Self::Place(p1, p2)
            }
            Kind::Pass => Self::Pass,
            Kind::ClaimWin => Self::ClaimWin(
                Point::decode(buf)?,
                Direction::from_u8(buf.try_get_u8().ok()?)?,
            ),
            Kind::Resign => Self::Resign,
            Kind::Request => Self::Request(Request::decode(buf)?),
            Kind::AcceptRequest => Self::AcceptRequest,
            Kind::DeclineRequest => Self::DeclineRequest,
        };
        (!buf.has_remaining()).then_some(msg)
    }
}

/// A server message.
#[derive(Clone, EnumDiscriminants)]
#[strum_discriminants(derive(FromRepr), name(ServerMessageKind), repr(u8), vis(pub(self)))]
pub enum ServerMessage {
    /// A new game was started.
    Started(GameId),
    /// The user was authenticated.
    Authenticated(Player),
    /// The game options were updated.
    Options(GameOptions),
    /// The entire record was updated.
    Record(Box<Record>),
    /// A move was made.
    Move(Move),
    /// The previous move was retracted.
    Retract,
    /// A player made a request.
    Request(Player, Request),
    /// A player accepted the opponent's request.
    AcceptRequest(Player),
    /// A player declined the opponent's request.
    DeclineRequest(Player),
}

impl Message for ServerMessage {
    fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u8(ServerMessageKind::from(&self) as u8);
        match self {
            Self::Started(id) => buf.put_i64(id.0),
            Self::Authenticated(player) => buf.put_u8(player as u8),
            Self::Options(options) => options.encode(buf),
            Self::Record(record) => record.encode(buf, RecordEncodingScheme::past()),
            Self::Move(mov) => mov.encode(buf, false),
            Self::Retract => {}
            Self::Request(player, req) => {
                buf.put_u8(player as u8);
                req.encode(buf);
            }
            Self::AcceptRequest(player) | Self::DeclineRequest(player) => buf.put_u8(player as u8),
        }
    }

    fn decode(buf: &mut &[u8]) -> Option<Self> {
        use ServerMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Started => Self::Started(GameId(buf.try_get_i64().ok()?)),
            Kind::Authenticated => Self::Authenticated(Player::from_u8(buf.try_get_u8().ok()?)?),
            Kind::Options => Self::Options(GameOptions::decode(buf)?),
            Kind::Record => Self::Record(Box::new(Record::decode(buf)?)),
            Kind::Move => Self::Move(Move::decode(buf, false)?),
            Kind::Retract => Self::Retract,
            Kind::Request => Self::Request(
                Player::from_u8(buf.try_get_u8().ok()?)?,
                Request::decode(buf)?,
            ),
            Kind::AcceptRequest => Self::AcceptRequest(Player::from_u8(buf.try_get_u8().ok()?)?),
            Kind::DeclineRequest => Self::DeclineRequest(Player::from_u8(buf.try_get_u8().ok()?)?),
        };
        (!buf.has_remaining()).then_some(msg)
    }
}
