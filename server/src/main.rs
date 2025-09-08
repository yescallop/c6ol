#![allow(missing_docs)]

use anyhow::Context;
use clap::Parser;
use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};
use tokio::{
    net::{TcpListener, TcpSocket},
    signal,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_PORT: u16 = 8086;

const DEFAULT_LISTEN: [SocketAddr; 2] = [
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
    SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), DEFAULT_PORT),
];

/// The server program for Connect6 Online
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Listen on the given socket addresses
    #[arg(long, name = "ADDR", num_args = 1.., default_values_t = DEFAULT_LISTEN)]
    listen: Vec<SocketAddr>,

    /// Serve files from the given directory
    #[arg(long, name = "DIR")]
    serve_dir: Option<PathBuf>,

    /// Open the given database file
    #[arg(long, name = "FILE")]
    db_file: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=trace", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let mut listeners = vec![];

    for addr in args.listen {
        listeners.push(listen(addr).with_context(|| format!("failed to listen on {addr}"))?);
        tracing::info!("listening on {addr}");
    }

    if let Some(path) = &args.serve_dir {
        tracing::info!("serving files from {}", path.display());
    }

    if let Some(path) = &args.db_file {
        tracing::info!("opening database at {}", path.display());
    } else {
        tracing::info!("opening in-memory database");
    };

    let shutdown_signal = shutdown_signal().context("failed to listen for shutdown signals")?;

    c6ol_server::run(listeners, args.serve_dir, args.db_file, shutdown_signal).await;
    Ok(())
}

fn listen(addr: SocketAddr) -> io::Result<TcpListener> {
    let socket = if addr.is_ipv4() {
        TcpSocket::new_v4()?
    } else {
        TcpSocket::new_v6()?
    };

    if addr.ip() == Ipv6Addr::UNSPECIFIED {
        socket2::SockRef::from(&socket).set_only_v6(false)?;
    }

    #[cfg(not(windows))]
    socket.set_reuseaddr(true)?;

    socket.bind(addr)?;
    socket.listen(1024)
}

#[cfg(unix)]
fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    let mut interrupt = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
    let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())?;

    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    })
}

#[cfg(windows)]
fn shutdown_signal() -> io::Result<impl Future<Output = ()>> {
    let mut ctrl_c = signal::windows::ctrl_c()?;

    Ok(async move {
        ctrl_c.recv().await;
    })
}
