//! WebSocket handling.

use crate::{
    manager::GameManager,
    protocol::{ClientMessage, ServerMessage},
};
use axum::{
    extract::{
        ws::{close_code, CloseFrame, Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use futures_util::{future, stream, SinkExt, StreamExt, TryStreamExt};
use std::convert::Infallible;

/// Handles a WebSocket upgrade.
#[remain::check]
pub async fn handle_websocket_upgrade(
    upgrade: WebSocketUpgrade,
    State(manager): State<GameManager>,
) -> Response {
    upgrade.on_upgrade(|mut socket| async move {
        // FIXME: Remove the else block when Rust 1.82 is out.
        let Err(e) = handle_websocket(&mut socket, manager).await else {
            return;
        };

        #[sorted]
        let code = match &e {
            Error::Axum(_) => close_code::ERROR,
            Error::Closed => return,
            Error::GameNotFound => close_code::NORMAL,
            Error::Lagged => close_code::AGAIN,
            Error::MalformedMessage => close_code::POLICY,
            Error::TextMessage => close_code::UNSUPPORTED,
            Error::UnexpectedMessage => close_code::POLICY,
            Error::WrongPasscode => close_code::NORMAL,
        };
        let msg = Message::Close(Some(CloseFrame {
            code,
            reason: e.to_string().into(),
        }));
        let _ = socket.send(msg).await;
    })
}

#[derive(Debug, thiserror::Error)]
#[remain::sorted]
enum Error {
    #[error("Axum error: {0}.")]
    Axum(#[from] axum::Error),
    #[error("WebSocket closed")]
    Closed,
    #[error("Game not found.")]
    GameNotFound,
    #[error("Game desynced due to server lag.")]
    Lagged,
    #[error("Malformed message.")]
    MalformedMessage,
    #[error("Text message not supported.")]
    TextMessage,
    #[error("Unexpected message.")]
    UnexpectedMessage,
    #[error("Wrong passcode.")]
    WrongPasscode,
}

// Handles a WebSocket connection.
async fn handle_websocket(
    socket: &mut WebSocket,
    manager: GameManager,
) -> Result<Infallible, Error> {
    let mut socket = socket
        .flat_map(|res| {
            stream::iter(match res {
                Ok(Message::Binary(data)) => match ClientMessage::deserialize(&data) {
                    Some(msg) => Some(Ok(msg)),
                    None => Some(Err(Error::MalformedMessage)),
                },
                Ok(Message::Text(_)) => Some(Err(Error::TextMessage)),
                Ok(_) => None,
                Err(e) => Some(Err(e.into())),
            })
        })
        .with(|msg: ServerMessage| future::ok::<_, axum::Error>(Message::Binary(msg.serialize())));

    let mut game;

    match socket.try_next().await?.ok_or(Error::Closed)? {
        ClientMessage::Start(passcode) => {
            game = manager.new_game().await;
            game.authenticate(passcode)
                .await
                .ok_or(Error::WrongPasscode)?;

            let msg = ServerMessage::Started {
                stone: game.stone().expect("should be authenticated"),
                game_id: Some(game.id()),
            };
            socket.send(msg).await?;
        }
        ClientMessage::Join(id) => {
            game = manager.find_game(id).await.ok_or(Error::GameNotFound)?;
        }
        _ => return Err(Error::UnexpectedMessage),
    }

    let mut sub = game.subscribe().await;
    for msg in sub.init_msgs {
        socket.send(msg).await?;
    }

    loop {
        tokio::select! {
            res = sub.msg_rx.recv() => {
                // The sender (in the game task) cannot have dropped
                // because we're holding a handle to the game.
                let msg = res.map_err(|_| Error::Lagged)?;
                socket.send(msg).await?;
            }
            res = socket.try_next() => {
                let msg = res?.ok_or(Error::Closed)?;
                match msg {
                    ClientMessage::Start(passcode) if game.stone().is_none() => {
                        game.authenticate(passcode).await.ok_or(Error::WrongPasscode)?;

                        let msg = ServerMessage::Started {
                            stone: game.stone().expect("should be authenticated"),
                            game_id: None,
                        };
                        socket.send(msg).await?;
                        continue;
                    }
                    ClientMessage::Start(_) | ClientMessage::Join(_) => {
                        return Err(Error::UnexpectedMessage);
                    }
                    _ => {}
                }
                game.play(msg).await;
            }
        }
    }
}
