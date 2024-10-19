use std::sync::Arc;
use tokio::sync::Notify;

pub struct Sender(Arc<Notify>);

impl Sender {
    pub fn send(self) {
        self.0.notify_waiters();
    }
}

#[derive(Clone)]
pub struct Receiver(Arc<Notify>);

impl Receiver {
    pub async fn recv(self) {
        self.0.notified().await;
    }
}

pub fn channel() -> (Sender, Receiver) {
    let notify = Arc::new(Notify::new());
    (Sender(notify.clone()), Receiver(notify))
}
