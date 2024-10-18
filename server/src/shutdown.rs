use tokio::sync::watch;

#[derive(Clone)]
pub(crate) struct Sender(watch::Sender<()>);

impl Sender {
    pub(crate) fn send(self) {
        let _ = self.0.send(());
    }
}

#[derive(Clone)]
pub(crate) struct Receiver(watch::Receiver<()>);

impl Receiver {
    pub(crate) async fn recv(mut self) {
        let _ = self.0.changed().await;
    }
}

pub(crate) fn channel() -> (Sender, Receiver) {
    let (tx, rx) = watch::channel(());
    (Sender(tx), Receiver(rx))
}
