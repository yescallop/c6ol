use tokio::sync::watch;

pub struct Sender(watch::Sender<()>);

impl Sender {
    pub fn send(self) {
        let _ = self.0.send(());
    }
}

#[derive(Clone)]
pub struct Receiver(watch::Receiver<()>);

impl Receiver {
    pub async fn recv(mut self) {
        let _ = self.0.changed().await;
    }
}

pub fn channel() -> (Sender, Receiver) {
    let (tx, rx) = watch::channel(());
    (Sender(tx), Receiver(rx))
}
