//! WebSocket handling.

use crate::{manager::GameManager, server::AppState};
use axum::{
    extract::{
        ws::{close_code, CloseFrame, Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use c6ol_core::protocol::{ClientMessage, ServerMessage};
use futures_util::{future, SinkExt, StreamExt};
use std::convert::Infallible;
use tokio::sync::broadcast::error::RecvError;

/// Handles a WebSocket upgrade.
#[remain::check]
pub async fn handle_websocket_upgrade(
    upgrade: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    upgrade.on_upgrade(|mut socket| async move {
        let err = tokio::select! {
            res = handle_websocket(&mut socket, state.manager) => {
                let Err(err) = res;
                err
            }
            () = state.shutdown_rx.requested() => {
                Error::Shutdown
            }
        };

        #[sorted]
        let code = match &err {
            Error::Axum(_) => close_code::ERROR,
            Error::Closed => return,
            Error::GameNotFound => close_code::NORMAL,
            Error::Lagged => close_code::AGAIN,
            Error::MalformedMessage => close_code::POLICY,
            Error::Shutdown => close_code::AWAY,
            Error::TextMessage => close_code::UNSUPPORTED,
            Error::UnexpectedMessage => close_code::POLICY,
            Error::WrongPasscode => close_code::NORMAL,
        };
        let msg = Message::Close(Some(CloseFrame {
            code,
            reason: err.to_string().into(),
        }));
        _ = socket.send(msg).await;
    })
}

#[derive(Debug, thiserror::Error)]
#[remain::sorted]
enum Error {
    #[error("Axum error: {0}.")]
    Axum(#[from] axum::Error),
    #[error("Connection closed.")]
    Closed,
    #[error("Game not found.")]
    GameNotFound,
    #[error("Game desynced due to server lag.")]
    Lagged,
    #[error("Malformed message.")]
    MalformedMessage,
    #[error("The server is going down.")]
    Shutdown,
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
        .filter_map(|res| {
            future::ready(match res {
                Ok(Message::Binary(data)) => match ClientMessage::decode(&data) {
                    Some(msg) => Some(Ok(msg)),
                    None => Some(Err(Error::MalformedMessage)),
                },
                Ok(Message::Text(_)) => Some(Err(Error::TextMessage)),
                Ok(_) => None,
                Err(err) => Some(Err(err.into())),
            })
        })
        .with(|msg: ServerMessage| {
            future::ok::<_, axum::Error>(Message::Binary(msg.encode().into()))
        });

    let mut game;

    match socket.next().await.ok_or(Error::Closed)?? {
        ClientMessage::Start(passcode) => {
            game = manager.new_game().await;
            game.authenticate(passcode)
                .await
                .ok_or(Error::WrongPasscode)?;

            let msg = ServerMessage::Started(
                game.stone().expect("should be authenticated"),
                Some(game.id()),
            );
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
                let msg = res.map_err(|err| match err {
                    RecvError::Closed => panic!("sender should be alive"),
                    RecvError::Lagged(_) => Error::Lagged,
                })?;
                socket.send(msg).await?;
            }
            opt = socket.next() => {
                let msg = opt.ok_or(Error::Closed)??;
                match msg {
                    ClientMessage::Start(passcode) if game.stone().is_none() => {
                        game.authenticate(passcode).await.ok_or(Error::WrongPasscode)?;

                        let msg = ServerMessage::Started(
                            game.stone().expect("should be authenticated"),
                            None,
                        );
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
