const DEFAULT_PORT: u16 = 8086;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = c6ol_server::run(DEFAULT_PORT).await {
        tracing::error!("IO error: {err}");
    }
}
