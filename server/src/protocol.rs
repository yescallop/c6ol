//! The WebSocket protocol.

use crate::game::{Game, Move, Stone};
use bytes::{Buf, BufMut, Bytes, BytesMut};

// Client messages.
const MSG_START: u8 = 0;
const MSG_JOIN: u8 = 1;
// Server messages.
const MSG_STARTED: u8 = 2;
const MSG_GAME: u8 = 3;
// Common messages.
const MSG_MOVE: u8 = 4;
const MSG_RETRACT: u8 = 5;

const PASSCODE_HASH_SIZE: usize = 32;
const GAME_ID_SIZE: usize = 10;

/// A client message.
#[derive(Clone, Copy)]
pub enum ClientMessage {
    /// When sent upon connection, requests to start a new game.
    /// When sent after `Join`, requests to authenticate.
    Start {
        /// The SHA-256 hash of the passcode.
        passcode_hash: [u8; PASSCODE_HASH_SIZE],
    },
    /// When sent upon connection, requests to join an existing game.
    Join {
        /// The game ID.
        game_id: [u8; GAME_ID_SIZE],
    },
    /// Requests a move.
    Move(Move),
    /// Requests to retract the previous move.
    Retract,
}

impl ClientMessage {
    /// Deserializes a client message from a buffer.
    pub fn deserialize(buf: &mut Bytes) -> Option<Self> {
        if !buf.has_remaining() {
            return None;
        }
        match buf.get_u8() {
            MSG_START => Some(Self::Start {
                passcode_hash: buf[..].try_into().ok()?,
            }),
            MSG_JOIN => Some(Self::Join {
                game_id: buf[..].try_into().ok()?,
            }),
            MSG_MOVE => {
                let mov = Move::deserialize(buf, false)?;
                if buf.has_remaining() {
                    return None;
                }
                Some(Self::Move(mov))
            }
            MSG_RETRACT => {
                if buf.has_remaining() {
                    return None;
                }
                Some(Self::Retract)
            }
            _ => None,
        }
    }
}

/// A server message.
#[derive(Clone, Copy)]
pub enum ServerMessage<'a> {
    /// The user is authenticated.
    /// Sent before `Game` if a new game is started.
    Started {
        /// The user's stone.
        stone: Stone,
        /// The game ID if a new game is started.
        game_id: Option<[u8; GAME_ID_SIZE]>,
    },
    /// The entire game is updated.
    Game(&'a Game),
    /// A move was made.
    Move(Move),
    /// The last move was retracted.
    Retract,
}

impl ServerMessage<'_> {
    /// Serializes a server message to a buffer.
    pub fn serialize(self, buf: &mut BytesMut) {
        match self {
            ServerMessage::Started { stone, game_id } => {
                buf.put_u8(MSG_STARTED);
                buf.put_u8(stone as u8);
                if let Some(id) = game_id {
                    buf.put_slice(&id);
                }
            }
            ServerMessage::Game(game) => {
                buf.put_u8(MSG_GAME);
                game.serialize(buf, false);
            }
            ServerMessage::Move(mov) => {
                buf.put_u8(MSG_MOVE);
                mov.serialize(buf, false);
            }
            ServerMessage::Retract => {
                buf.put_u8(MSG_RETRACT);
            }
        }
    }
}
