//! Stopping remote access closes connections that are already open: a client
//! watching activity over TLS stops promptly and sees nothing produced later.

use std::time::{Duration, Instant};

use tonic::{Request, Streaming};

use super::*;
use crate::rpc::proto::runtime_server::Runtime;

/// A prompt stop includes the grace period for in-flight calls.
const PROMPT: Duration = Duration::from_secs(3);

enum Stop {
    Disable,
    Replace,
    Shutdown,
}

async fn produce_event(service: &RuntimeService) {
    let request = Request::new(proto::UnloadModelRequest { selector: "absent".to_owned() });
    drop(Runtime::unload_model(service, request).await);
}

async fn next_event(
    stream: &mut Streaming<proto::ActivityEvent>,
) -> crate::error::Result<proto::ActivityEvent> {
    let event = tokio::time::timeout(PROMPT, stream.message())
        .await
        .map_err(|_| crate::error::Error::Remote("no activity event arrived".to_owned()))??;
    Ok(event.expect("the stream is open"))
}

async fn closes_an_open_stream(stop: Stop) -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let status = remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let listening = status.listening.expect("listener running");
    let (_, key) = remote.create_key("watcher", Scope::Use)?;
    let address = listening.address.to_string();
    let mut client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;
    let mut stream = client
        .watch_activity(proto::WatchActivityRequest { include_history: false })
        .await?
        .into_inner();
    produce_event(&service).await;
    next_event(&mut stream).await?;
    let mut before = 0_usize;
    while let Ok(Ok(Some(_))) =
        tokio::time::timeout(Duration::from_millis(100), stream.message()).await
    {
        before += 1;
    }
    assert!(before < 16, "the event stream settles before stopping");

    let started = Instant::now();
    match stop {
        Stop::Disable => drop(remote.disable().await?),
        Stop::Replace => drop(remote.enable(Some("127.0.0.1:0"), service.clone()).await?),
        Stop::Shutdown => remote.shutdown().await,
    }
    assert!(started.elapsed() < PROMPT, "stopping took {:?}", started.elapsed());
    produce_event(&service).await;

    let ended = tokio::time::timeout(PROMPT, stream.message())
        .await
        .expect("the open stream ends promptly");
    assert!(
        !matches!(ended, Ok(Some(_))),
        "an event produced after the listener stopped reached the client: {ended:?}"
    );
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn disabling_closes_open_streams() -> crate::error::Result<()> {
    closes_an_open_stream(Stop::Disable).await
}

#[tokio::test]
async fn replacing_the_listener_closes_open_streams() -> crate::error::Result<()> {
    closes_an_open_stream(Stop::Replace).await
}

#[tokio::test]
async fn shutdown_closes_open_streams() -> crate::error::Result<()> {
    closes_an_open_stream(Stop::Shutdown).await
}
