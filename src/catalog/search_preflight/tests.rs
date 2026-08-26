use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;

#[tokio::test]
async fn scheduler_respects_the_concurrency_limit() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let requests = (0..8)
        .map(|index| {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                let _previous = maximum.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _previous = active.fetch_sub(1, Ordering::SeqCst);
                index
            }
        })
        .collect();

    let completed = run_bounded(requests, 2, Duration::from_secs(1)).await;

    assert_eq!(completed.len(), 8);
    assert!(maximum.load(Ordering::SeqCst) <= 2);
}

#[tokio::test]
async fn scheduler_stops_at_the_shared_budget() {
    let requests = (0..4)
        .map(|index| async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            index
        })
        .collect();

    let completed = run_bounded(requests, 2, Duration::from_millis(10)).await;

    assert_eq!(completed, Vec::<usize>::new());
}
