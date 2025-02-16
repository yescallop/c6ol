use crate::{db, game, shutdown, ws};
use axum::{routing::get, Router};
use std::{
    future::{Future, IntoFuture},
    iter,
    path::PathBuf,
};
use tokio::{net::TcpListener, task::JoinSet};
use tower_http::services::ServeDir;

/// Shared state for WebSocket handlers.
#[derive(Clone)]
pub struct AppState {
    pub shutdown_rx: shutdown::Receiver,
    pub game_manager: game::GameManager,
}

/// Runs the server.
pub async fn run(
    listeners: Vec<TcpListener>,
    serve_dir: Option<PathBuf>,
    db_file: Option<PathBuf>,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) {
    // Set up graceful shutdown, on which the following events happen:
    //
    // - All WebSocket handlers are cancelled, dropping all `GameManager`s
    //   (except the one shared by the axum servers) and `Game`s.
    // - The axum servers shut down after all connections are closed,
    //   dropping the last `GameManager`.
    // - All game tasks finish after no `Game`s are alive.
    // - The game manager task finishes after no `GameManager`s are alive
    //   and all game tasks finish.
    // - The database manager task finishes after the game manager task.
    let (shutdown_tx, shutdown_rx) = shutdown::channel();
    tokio::spawn(async move {
        shutdown_signal.await;
        shutdown_tx.request();
    });

    let (db_manager, db_manager_task) = db::manager(db_file);
    let (game_manager, game_manager_fut) = game::manager(db_manager);
    let game_manager_task = tokio::spawn(game_manager_fut);

    let app_state = AppState {
        shutdown_rx: shutdown_rx.clone(),
        game_manager,
    };

    let mut app = Router::new()
        .route("/ws", get(ws::handle_websocket_upgrade))
        .with_state(app_state);

    if let Some(path) = serve_dir {
        app = app.fallback_service(ServeDir::new(path));
    }

    let mut server_tasks = JoinSet::new();

    for ((app, shutdown_rx), listener) in iter::repeat((app, shutdown_rx)).zip(listeners) {
        server_tasks.spawn(
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_rx.requested())
                .into_future(),
        );
    }

    while let Some(res) = server_tasks.join_next().await {
        match res {
            Ok(Ok(())) => {}
            Ok(Err(err)) => tracing::error!("server task returned error: {err}"),
            Err(err) => tracing::error!("server task panicked: {err}"),
        }
    }

    if let Err(err) = game_manager_task.await {
        tracing::error!("game manager task panicked: {err}");
    }

    if let Err(err) = db_manager_task.await {
        tracing::error!("database manager task panicked: {err}");
    }
}
