//! WebSocket protocol.

use crate::game::{Direction, Move, Player, Point, Record, RecordEncodeMethod, Stone};
use bytes::{Buf, BufMut};
use serde::{Deserialize, Serialize};
use std::{fmt, iter, mem, str};
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

const BASE62_LUT: &[u8] = &{
    let mut out = [0xff; 256];
    let mut i = 0;
    while i < 62 {
        out[BASE62_ALPHABET[i] as usize] = i as u8;
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
            if x > 127 {
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
        let mut i = 10;
        while n != 0 {
            buf[i] = BASE62_ALPHABET[(n % 62) as usize];
            n /= 62;
            i -= 1;
        }
        f.write_str(str::from_utf8(&buf).unwrap())
    }
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

    /// Encodes the options to a buffer.
    pub fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u8(self.swapped as u8);
    }

    /// Encodes the options to a new buffer.
    #[must_use]
    pub fn encode_to_vec(self) -> Vec<u8> {
        let mut buf = vec![];
        self.encode(&mut buf);
        buf
    }

    /// Decodes options from a buffer.
    #[must_use]
    pub fn decode(buf: &mut &[u8]) -> Option<Self> {
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

impl Request {
    /// Encodes the request to a buffer.
    pub fn encode(self, buf: &mut Vec<u8>) {
        buf.put_u8(RequestKind::from(self) as u8);
        match self {
            Self::Draw | Self::Retract => {}
            Self::Reset(options) => options.encode(buf),
        }
    }

    /// Encodes the request to a new buffer.
    #[must_use]
    pub fn encode_to_vec(self) -> Vec<u8> {
        let mut buf = vec![];
        self.encode(&mut buf);
        buf
    }

    /// Decodes a request from a buffer.
    #[must_use]
    pub fn decode(buf: &mut &[u8]) -> Option<Self> {
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
    Start(GameOptions, Passcode),
    /// Requests to join an existing game.
    Join(GameId),
    /// Requests to authenticate.
    Authenticate(Passcode),
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

impl ClientMessage {
    /// Encodes the client message to a new buffer.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut buf = vec![ClientMessageKind::from(&self) as u8];
        match self {
            Self::Start(options, passcode) => {
                options.encode(&mut buf);
                buf.put_slice(&passcode);
            }
            Self::Join(game_id) => buf.put_i64(game_id.0),
            Self::Authenticate(passcode) => buf.put_slice(&passcode),
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
            Self::Request(req) => req.encode(&mut buf),
            Self::AcceptRequest | Self::DeclineRequest => {}
        }
        buf
    }

    /// Decodes a client message from a buffer.
    #[must_use]
    pub fn decode(mut buf: &[u8]) -> Option<Self> {
        use ClientMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Start => Self::Start(
                GameOptions::decode(&mut buf)?,
                Box::from(mem::take(&mut buf)),
            ),
            Kind::Join => Self::Join(GameId(buf.try_get_i64().ok()?)),
            Kind::Authenticate => Self::Authenticate(Box::from(mem::take(&mut buf))),
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
            Kind::Request => Self::Request(Request::decode(&mut buf)?),
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

impl ServerMessage {
    /// Encodes the server message to a new buffer.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut buf = vec![ServerMessageKind::from(&self) as u8];
        match self {
            Self::Started(id) => buf.put_i64(id.0),
            Self::Authenticated(player) => buf.put_u8(player as u8),
            Self::Options(options) => options.encode(&mut buf),
            Self::Record(record) => record.encode(&mut buf, RecordEncodeMethod::Past),
            Self::Move(mov) => mov.encode(&mut buf, true),
            Self::Retract => {}
            Self::Request(player, req) => {
                buf.put_u8(player as u8);
                req.encode(&mut buf);
            }
            Self::AcceptRequest(player) | Self::DeclineRequest(player) => buf.put_u8(player as u8),
        }
        buf
    }

    /// Decodes a server message from a buffer.
    #[must_use]
    pub fn decode(mut buf: &[u8]) -> Option<Self> {
        use ServerMessageKind as Kind;

        let msg = match Kind::from_repr(buf.try_get_u8().ok()?)? {
            Kind::Started => Self::Started(GameId(buf.try_get_i64().ok()?)),
            Kind::Authenticated => Self::Authenticated(Player::from_u8(buf.try_get_u8().ok()?)?),
            Kind::Options => Self::Options(GameOptions::decode(&mut buf)?),
            Kind::Record => Self::Record(Box::new(Record::decode(&mut buf)?)),
            Kind::Move => Self::Move(Move::decode(&mut buf, false)?),
            Kind::Retract => Self::Retract,
            Kind::Request => Self::Request(
                Player::from_u8(buf.try_get_u8().ok()?)?,
                Request::decode(&mut buf)?,
            ),
            Kind::AcceptRequest => Self::AcceptRequest(Player::from_u8(buf.try_get_u8().ok()?)?),
            Kind::DeclineRequest => Self::DeclineRequest(Player::from_u8(buf.try_get_u8().ok()?)?),
        };
        (!buf.has_remaining()).then_some(msg)
    }
}
