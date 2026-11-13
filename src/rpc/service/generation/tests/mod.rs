mod runtime;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use runtime::{Blocked, Gate};
use tokio_stream::StreamExt;

use super::*;
use crate::{
    adapters::native::NativeRuntime,
    application::Application,
    config::{AppConfig, Paths, Store},
    remote::RemoteAccess,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A service whose generations are held in model resolution, before output.
fn held_service() -> (RuntimeService, Gate, std::path::PathBuf) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("mirmir-rpc-generation-{}-{id}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let telemetry_file = paths.telemetry_file.clone();
    let native = Arc::new(NativeRuntime::new(&AppConfig::default(), Store::new(paths.clone())));
    let (blocked, gate) = Blocked::new();
    let application =
        Application::from_ports(Arc::new(blocked), native.clone(), native, telemetry_file);
    let service =
        RuntimeService::from_application(application, RemoteAccess::new(Store::new(paths)));
    (service, gate, root)
}

fn request() -> proto::GenerateRequest {
    proto::GenerateRequest {
        model: "held".to_owned(),
        prompt: "hello".to_owned(),
        ..proto::GenerateRequest::default()
    }
}

/// Blocks the test until the held generation reaches model resolution; the
/// generation runs on the blocking pool, so the test runtime may wait here.
fn entered(gate: &Gate) {
    gate.entered
        .recv_timeout(Duration::from_secs(5))
        .expect("generation reaches model resolution");
}

#[tokio::test]
async fn a_disconnect_before_output_cancels_the_generation() -> crate::error::Result<()> {
    let (service, gate, root) = held_service();
    service.restore_active_models()?;
    let session = service.application.start_generation("held");
    let cancellation = session.cancellation();
    let stream = spawn(service, request(), session);
    entered(&gate);
    assert!(!cancellation.is_cancelled());

    drop(stream);
    tokio::time::timeout(Duration::from_secs(1), async {
        while !cancellation.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("dropping the client cancels the held generation");
    let _released = gate.release.send(());
    drop(std::fs::remove_dir_all(root));
    Ok(())
}

#[tokio::test]
async fn a_completed_generation_ends_the_stream_without_cancelling() -> crate::error::Result<()> {
    let (service, gate, root) = held_service();
    service.restore_active_models()?;
    let _released = gate.release.send(());
    let session = service.application.start_generation("held");
    let cancellation = session.cancellation();
    let mut stream = spawn(service, request(), session);

    let events = tokio::time::timeout(Duration::from_secs(5), async {
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }
        events
    })
    .await
    .expect("the stream reaches EOF once the producer completes");
    assert!(matches!(
        events.first(),
        Some(Ok(proto::GenerateEvent {
            event: Some(proto::generate_event::Event::Started(_))
        }))
    ));
    assert!(
        matches!(events.last(), Some(Err(_))),
        "the producer's outcome is the last event"
    );
    assert_eq!(events.len(), 2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!cancellation.is_cancelled(), "a completed stream is not cancelled");
    drop(std::fs::remove_dir_all(root));
    Ok(())
}
