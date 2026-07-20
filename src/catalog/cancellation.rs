use std::{future::Future, time::Duration};

use libmir::CancellationToken;

pub(super) async fn wait<T>(
    future: impl Future<Output = T>,
    token: &CancellationToken,
) -> Option<T> {
    tokio::pin!(future);
    let mut poll = tokio::time::interval(Duration::from_millis(50));
    loop {
        tokio::select! {
            biased;
            output = &mut future => return Some(output),
            _ = poll.tick() => {
                if token.is_cancelled() {
                    return None;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drops_pending_work_after_cancellation() {
        let token = CancellationToken::new();
        token.cancel();
        let result = wait(std::future::pending::<()>(), &token).await;
        assert!(result.is_none());
    }
}
