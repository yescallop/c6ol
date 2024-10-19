use std::{io, net::Ipv6Addr};
use tokio::{net::TcpSocket, signal};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const BIND_PORT: u16 = 8086;
const STATIC_ROOT: &str = "../client/dist";

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let listener = {
        let socket = TcpSocket::new_v6()?;
        socket2::SockRef::from(&socket).set_only_v6(false)?;

        #[cfg(not(windows))]
        socket.set_reuseaddr(true)?;

        socket.bind((Ipv6Addr::UNSPECIFIED, BIND_PORT).into())?;
        socket.listen(1024)?
    };
    tracing::info!("listening on {}", listener.local_addr()?);

    c6ol_server::run(listener, STATIC_ROOT, shutdown_signal()).await
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
