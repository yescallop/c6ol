/// Convenience macro for command execution.
macro_rules! execute {
    ($cmd_tx:expr, $variant:path, $($args:expr),*) => {{
        let (tx, rx) = oneshot::channel();
        $cmd_tx.send($variant(tx, $($args),*)).await.expect("receiver should be alive");
        rx.await.expect("command should return")
    }};
    ($cmd_tx:expr, $cmd:expr) => {
        $cmd_tx.send($cmd).await.expect("receiver should be alive")
    };
}

pub(crate) use execute;
