#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = c6ol_server::run().await {
        tracing::error!("IO error: {err}");
    }
}
