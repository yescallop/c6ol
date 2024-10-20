use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::Notify;

struct Shared {
    notify: Notify,
    requested: AtomicBool,
}

pub struct Sender(Arc<Shared>);

impl Sender {
    pub fn request(&self) {
        self.0.requested.store(true, Ordering::Relaxed);
        self.0.notify.notify_waiters();
    }
}

#[derive(Clone)]
pub struct Receiver(Arc<Shared>);

impl Receiver {
    pub async fn requested(self) {
        let notified = self.0.notify.notified();
        if !self.0.requested.load(Ordering::Relaxed) {
            notified.await;
        }
    }
}

pub fn channel() -> (Sender, Receiver) {
    let shared = Arc::new(Shared {
        notify: Notify::new(),
        requested: AtomicBool::new(false),
    });
    (Sender(shared.clone()), Receiver(shared))
}
