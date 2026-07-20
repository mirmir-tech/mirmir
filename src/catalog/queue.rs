use std::sync::Arc;

use libmir::CancellationToken;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::cancellation;
use crate::error::{Error, Result};

#[derive(Clone)]
pub(super) struct TransferQueue {
    slots: Arc<Semaphore>,
}

impl TransferQueue {
    pub fn serial() -> Self {
        Self { slots: Arc::new(Semaphore::new(1)) }
    }

    pub async fn acquire(&self, cancellation: &CancellationToken) -> Result<OwnedSemaphorePermit> {
        let pending = self.slots.clone().acquire_owned();
        cancellation::wait(pending, cancellation)
            .await
            .ok_or(Error::Cancelled)?
            .map_err(|_| Error::Config("download queue is closed".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queued_transfer_can_be_cancelled() -> Result<()> {
        let queue = TransferQueue::serial();
        let first = queue.acquire(&CancellationToken::new()).await?;
        let cancellation = CancellationToken::new();
        let queued = tokio::spawn({
            let queue = queue.clone();
            let cancellation = cancellation.clone();
            async move { queue.acquire(&cancellation).await }
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        let queued_result = queued.await?;
        drop(first);
        assert!(matches!(&queued_result, Err(Error::Cancelled)));
        drop(queued_result);
        Ok(())
    }

    #[tokio::test]
    async fn queued_transfers_start_in_fifo_order() -> Result<()> {
        let queue = TransferQueue::serial();
        let first = queue.acquire(&CancellationToken::new()).await?;
        let (started, mut order) = tokio::sync::mpsc::channel(2);
        for index in [2, 3] {
            let queue = queue.clone();
            let started = started.clone();
            drop(tokio::spawn(async move {
                let permit = queue.acquire(&CancellationToken::new()).await.expect("queue open");
                started.send(index).await.expect("receiver open");
                drop(permit);
            }));
            tokio::task::yield_now().await;
        }
        drop(started);
        drop(first);
        assert_eq!(order.recv().await, Some(2));
        assert_eq!(order.recv().await, Some(3));
        Ok(())
    }
}
