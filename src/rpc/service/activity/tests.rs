use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    rpc::RuntimeService,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn dropped_idle_watchers_release_their_producers() -> Result<()> {
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-activity-watch-{}-{unique}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("runtime"));
    let service = RuntimeService::new(&AppConfig::default(), Store::new(paths));
    let application = &service.application;
    let baseline = application.activity_subscribers();

    let streams: Vec<_> = (0..32).map(|_| super::watch(application, false)).collect();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(application.activity_subscribers(), baseline + 32);
    drop(streams);

    // No activity is published: the producers must notice the closed streams.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while application.activity_subscribers() > baseline {
        assert!(tokio::time::Instant::now() < deadline, "idle producers outlived their streams");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(std::fs::remove_dir_all(root));
    Ok(())
}
