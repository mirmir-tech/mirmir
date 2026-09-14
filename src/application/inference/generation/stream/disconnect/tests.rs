use std::time::Duration;

use super::*;

#[tokio::test]
async fn disconnect_cancels_without_waiting_for_output() -> Result<(), tokio::time::error::Elapsed>
{
    let cancellation = CancellationToken::new();
    let unrelated = CancellationToken::new();
    let (sender, receiver) = mpsc::channel::<u8>(1);
    let (other_sender, _other_receiver) = mpsc::channel::<u8>(1);
    let _watch = Watch::new(sender, cancellation.clone());
    let _other = Watch::new(other_sender, unrelated.clone());
    drop(receiver);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !cancellation.is_cancelled() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(!unrelated.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn completed_generation_does_not_leave_a_sender_or_cancel()
-> Result<(), tokio::time::error::Elapsed> {
    let cancellation = CancellationToken::new();
    let (sender, mut receiver) = mpsc::channel::<u8>(1);
    let watch = Watch::new(sender, cancellation.clone());
    drop(watch);
    assert_eq!(tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await?, None);
    assert!(!cancellation.is_cancelled());
    Ok(())
}
