use axum::{routing::get, Router};
use c6ol_server::{handle_websocket_upgrade, manager::GameManager};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/ws", get(handle_websocket_upgrade))
        .with_state(GameManager::spawn())
        .fallback_service(ServeDir::new("../client/dist"));

    let listener_v4 = TcpListener::bind("0.0.0.0:8086").await.unwrap();
    tracing::debug!("listening on {}", listener_v4.local_addr().unwrap());

    let listener_v6 = TcpListener::bind("[::]:8086").await.unwrap();
    tracing::debug!("listening on {}", listener_v6.local_addr().unwrap());

    tokio::try_join!(
        axum::serve(listener_v4, app.clone()),
        axum::serve(listener_v6, app)
    )
    .unwrap();
}
