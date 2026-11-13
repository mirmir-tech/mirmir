use libmir::CancellationToken;
use tokio::{sync::mpsc, task::JoinHandle};

/// Cancels a generation as soon as its consumer goes away, independently of
/// output; dropping the watch retires it so it neither keeps the sender alive
/// nor cancels a generation that already finished.
pub struct Watch(JoinHandle<()>);

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
