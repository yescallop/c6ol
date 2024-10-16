//! WebSocket handling.

use crate::{
    manager::GameManager,
    protocol::{ClientMessage, ServerMessage},
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::Response,
};
use futures_util::{future, SinkExt, StreamExt};

/// Handles WebSocket upgrades.
pub async fn handle_websocket_upgrade(
    upgrade: WebSocketUpgrade,
    State(manager): State<GameManager>,
) -> Response {
    upgrade.on_upgrade(|socket| async {
        handle_websocket(socket, manager).await;
    })
}

// This function deals with a single websocket connection, i.e., a single
// connected client / user, for which we will spawn two independent tasks (for
// receiving / sending chat messages).
async fn handle_websocket(socket: WebSocket, manager: GameManager) -> Option<()> {
    // By splitting, we can send and receive at the same time.
    let (msg_tx, msg_rx) = socket.split();

    let mut msg_tx = msg_tx.with(|msg: ServerMessage| {
        future::ready(Ok::<_, axum::Error>(Message::Binary(msg.serialize())))
    });
    let mut msg_rx = msg_rx
        .map(|res| match res {
            Ok(Message::Binary(data)) => match ClientMessage::deserialize(&data) {
                Some(msg) => Ok(Some(msg)),
                None => Err(()),
            },
            Ok(Message::Text(_)) | Err(_) => Err(()),
            _ => Ok(None),
        })
        .take_while(|res| future::ready(res.is_ok()))
        .filter_map(|res| future::ready(res.unwrap()));

    let mut game;

    match msg_rx.next().await? {
        ClientMessage::Start(passcode) => {
            game = manager.new_game().await;
            game.authenticate(passcode).await?;

            let msg = ServerMessage::Started {
                stone: game.stone().unwrap(),
                game_id: Some(game.id()),
            };
            msg_tx.send(msg).await.ok()?;
        }
        ClientMessage::Join(id) => {
            game = manager.find_game(id).await?;
        }
        _ => return None,
    }

    let mut sub = game.subscribe().await;
    for msg in sub.init_msgs {
        msg_tx.send(msg).await.ok()?;
    }

    loop {
        tokio::select! {
            res = sub.msg_rx.recv() => {
                // Bail out if the receiver lagged.
                let msg = res.ok()?;
                msg_tx.send(msg).await.ok()?;
            }
            opt = msg_rx.next() => {
                // Bail out if the client disconnected.
                let msg = opt?;
                match msg {
                    ClientMessage::Start(passcode) if game.stone().is_none() => {
                        game.authenticate(passcode).await?;

                        let msg = ServerMessage::Started {
                            stone: game.stone().unwrap(),
                            game_id: None,
                        };
                        msg_tx.send(msg).await.ok();
                        continue;
                    }
                    ClientMessage::Start(_) | ClientMessage::Join(_) => return None,
                    _ => {}
                }
                game.play(msg).await;
            }
        }
    }
}
