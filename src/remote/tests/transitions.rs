//! Enabling and disabling remote access are single transitions: the persisted
//! setting, the status and the reachable listener always agree afterwards.

use std::{path::Path, time::Duration};

use super::*;

/// Whether a fresh authenticated TLS client can call the server at `address`.
async fn reachable(address: &str, key: &str, fingerprint: Fingerprint) -> bool {
    match rpc::connect_remote(&target(address, key, fingerprint)).await {
        Ok(mut client) => client.telemetry(proto::TelemetryRequest {}).await.is_ok(),
        Err(_) => false,
    }
}

/// Checks that the persisted setting, the status and the network agree.
async fn agrees(service: &RuntimeService, root: &Path, key: &str) -> crate::error::Result<Status> {
    let status = service.remote().status().await?;
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let persisted = Store::new(paths).load()?.server.remote_bind;
    assert_eq!(status.bind, persisted, "the status reports the persisted setting");
    assert_eq!(
        status.bind.is_some(),
        status.listening.is_some(),
        "the listener runs exactly when remote access is on: {status:?}"
    );
    if let Some(listening) = &status.listening {
        let address = listening.address.to_string();
        assert!(reachable(&address, key, listening.fingerprint).await, "{address} answers");
    }
    Ok(status)
}

/// Makes every configuration write fail while reads still work: the writer
/// creates the models directory first, and a file stands in its way.
fn block_writes(config: &Path) -> crate::error::Result<()> {
    let models = config.join("models");
    drop(std::fs::remove_dir_all(&models));
    std::fs::create_dir_all(config)?;
    Ok(std::fs::write(models, b"")?)
}

fn allow_writes(config: &Path) -> crate::error::Result<()> {
    Ok(std::fs::remove_file(config.join("models"))?)
}

#[tokio::test]
async fn an_enable_racing_a_slow_disable_runs_after_it() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let first = remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let listening = first.listening.expect("listener running");
    let (_, key) = remote.create_key("racer", Scope::Use)?;
    let address = listening.address.to_string();
    let mut client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;
    let _stream = client
        .watch_activity(proto::WatchActivityRequest { include_history: false })
        .await?
        .into_inner();

    let disabling = tokio::spawn({
        let remote = remote.clone();
        async move { remote.disable().await }
    });
    while remote.status().await?.listening.is_some() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let enabled = remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let disabled = disabling.await.expect("disable task")?;

    assert!(disabled.bind.is_none() && disabled.listening.is_none(), "{disabled:?}");
    assert!(enabled.bind.is_some() && enabled.listening.is_some(), "{enabled:?}");
    let last = agrees(&service, &root, &key).await?;
    assert!(last.listening.is_some(), "the later enable wins");
    assert!(
        !reachable(&address, &key, listening.fingerprint).await,
        "the old listener is gone"
    );

    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn an_enable_that_cannot_persist_leaves_no_listener() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let (_, key) = remote.create_key("probe", Scope::Use)?;
    let free = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.to_string();
    let config = root.join("config");
    block_writes(&config)?;
    assert!(remote.enable(Some(&free), service.clone()).await.is_err());
    let status = agrees(&service, &root, &key).await?;
    assert!(status.listening.is_none());
    assert!(
        tokio::net::TcpStream::connect(&free).await.is_err(),
        "nothing listens on {free}"
    );
    allow_writes(&config)?;
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn a_disable_that_cannot_persist_keeps_the_listener() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let (_, key) = remote.create_key("probe", Scope::Use)?;
    remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let config = root.join("config");
    block_writes(&config)?;
    assert!(remote.disable().await.is_err());
    let status = agrees(&service, &root, &key).await?;
    assert!(status.listening.is_some(), "remote access stays on as persisted");
    allow_writes(&config)?;
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn no_listener_starts_after_shutdown() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    remote.shutdown().await;
    assert!(remote.enable(Some("127.0.0.1:0"), service.clone()).await.is_err());
    assert!(remote.status().await?.listening.is_none());
    drop(std::fs::remove_dir_all(root));
    Ok(())
}
