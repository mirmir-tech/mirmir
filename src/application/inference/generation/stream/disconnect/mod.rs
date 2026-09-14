use libmir::CancellationToken;
use tokio::{sync::mpsc, task::JoinHandle};

pub(super) struct Watch(JoinHandle<()>);

impl Watch {
    pub(super) fn new<T: Send + 'static>(
        sender: mpsc::Sender<T>,
        cancellation: CancellationToken,
    ) -> Self {
        Self(tokio::spawn(async move {
            sender.closed().await;
            cancellation.cancel();
        }))
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests;
