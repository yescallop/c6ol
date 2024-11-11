#![allow(missing_docs)]

use anyhow::Context;
use clap::Parser;
use std::{
    future::Future,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};
use tokio::{net::TcpSocket, signal};
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
    #[arg(long, name = "PATH")]
    serve_dir: Option<PathBuf>,
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

    let listeners = args
        .listen
        .into_iter()
        .map(|addr| {
            tracing::info!("listening on {addr}");

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
        })
        .collect::<io::Result<Vec<_>>>()
        .context("failed to listen on previous address")?;

    let shutdown_signal = shutdown_signal().context("failed to listen for shutdown signals")?;

    let serve_dir = if let Some(path) = &args.serve_dir {
        let path = path
            .canonicalize()
            .ok()
            .filter(|path| path.is_dir())
            .context("argument to `serve-dir` is not pointing to a valid directory")?;
        tracing::info!("serving files from {}", path.display());
        Some(path)
    } else {
        None
    };

    c6ol_server::run(listeners, serve_dir.as_deref(), shutdown_signal).await;
    Ok(())
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
