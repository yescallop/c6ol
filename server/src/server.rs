use crate::{manager, shutdown, ws};
use axum::{routing::get, Router};
use std::{future::Future, io};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

/// Shared state for WebSocket handlers.
#[derive(Clone)]
pub(crate) struct AppState {
    pub shutdown_rx: shutdown::Receiver,
    pub manager: manager::GameManager,
}

/// Runs the server.
///
/// # Errors
///
/// Returns `Err` if an error occurred when serving.
pub async fn run(
    listener: TcpListener,
    static_root: &str,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    // Set up graceful shutdown, on which the following events happen:
    //
    // - All WebSocket handlers are cancelled, dropping all `GameManager`s
    //   (except the one in the axum server) and `Game`s.
    // - The axum server shuts down after all connections are closed,
    //   dropping the last `GameManager`.
    // - All game tasks finish after no `Game`s are alive.
    // - The game manager task finishes after no `GameManager`s are alive
    //   and all game tasks finish.
    let (shutdown_tx, shutdown_rx) = shutdown::channel();
    tokio::spawn(async move {
        shutdown_signal.await;
        shutdown_tx.send();
    });

    let (manager, manager_fut) = manager::create();
    let manager_task = tokio::spawn(manager_fut);

    let app_state = AppState {
        shutdown_rx: shutdown_rx.clone(),
        manager,
    };

    let app = Router::new()
        .route("/ws", get(ws::handle_websocket_upgrade))
        .with_state(app_state)
        .fallback_service(ServeDir::new(static_root));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_rx.recv())
        .await?;

    manager_task.await.expect("manager task should not panic");
    Ok(())
}
