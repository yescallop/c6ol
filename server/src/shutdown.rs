use tokio::sync::watch;

pub struct Sender(watch::Sender<()>);

impl Sender {
    /// Requests a shutdown.
    pub fn request(&self) {
        self.0.send_replace(());
    }
}

#[derive(Clone)]
pub struct Receiver(watch::Receiver<()>);

impl Receiver {
    /// Waits unless or until a shutdown is requested or the sender is dropped.
    pub async fn requested(mut self) {
        _ = self.0.changed().await;
    }
}

pub fn channel() -> (Sender, Receiver) {
    let (tx, rx) = watch::channel(());
    (Sender(tx), Receiver(rx))
}
