use crate::{manager, shutdown, ws};
use axum::{routing::get, Router};
use futures_util::FutureExt;
use std::{io, net::Ipv6Addr};
use tokio::{net::TcpSocket, signal};
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
/// Returns `Err` if an error occurred when listening or serving.
pub async fn run(port: u16) -> io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Set up graceful shutdown, on which the following sequence of events happen.
    //
    // - All WebSocket handlers are cancelled. Shutdown messages are sent to clients.
    //   All `Game`s and `GameManager`s are dropped, except the ones in Axum servers.
    // - Axum servers shut down, dropping the remaining `GameManager`s.
    // - All game futures run to completion because no `Game`s are alive, followed by
    //   the game manager future because no `GameManager`s or game futures are alive.
    let (shutdown_tx, shutdown_rx) = shutdown::channel();
    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_tx.send();
    });

    let (manager, manager_fut) = manager::create();

    let app_state = AppState {
        shutdown_rx: shutdown_rx.clone(),
        manager,
    };

    let app = Router::new()
        .route("/ws", get(ws::handle_websocket_upgrade))
        .with_state(app_state)
        .fallback_service(ServeDir::new("../client/dist"));

    let listener = {
        let socket = TcpSocket::new_v6()?;
        socket2::SockRef::from(&socket).set_only_v6(false)?;
        #[cfg(not(windows))]
        socket.set_reuseaddr(true)?;
        socket.bind((Ipv6Addr::UNSPECIFIED, port).into())?;
        socket.listen(1024)?
    };
    tracing::debug!("listening on {}", listener.local_addr()?);

    tokio::try_join!(
        manager_fut.map(Ok),
        axum::serve(listener, app).with_graceful_shutdown(shutdown_rx.recv()),
    )?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
