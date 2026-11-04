use super::*;

#[tokio::test]
async fn shutdown_waits_for_worker_to_release_its_runtime_handle() -> Result<()> {
    let (shutdown, receiver) = mpsc::channel();
    let (finishing, started) = tokio::sync::oneshot::channel();
    let (release, gate) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        receiver.recv().expect("shutdown signal");
        let _sent = finishing.send(());
        gate.recv().expect("release worker");
        Ok(())
    });
    let sampler = Sampler {
        shutdown: Some(shutdown),
        worker: Some(worker),
    };
    let task = tokio::spawn(sampler.shutdown());
    started.await.expect("worker finishing");
    let detached = task.is_finished();
    release.send(()).expect("release blocked worker");
    task.await??;
    assert!(!detached, "shutdown returned before the sampler released its state");
    Ok(())
}

#[tokio::test]
async fn shutdown_propagates_history_flush_failure() {
    let worker = std::thread::spawn(|| Err(crate::error::Error::Config("flush failed".into())));
    let sampler = Sampler { shutdown: None, worker: Some(worker) };
    assert!(sampler.shutdown().await.is_err());
}
