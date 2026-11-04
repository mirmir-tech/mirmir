use super::*;

#[tokio::test]
async fn stalled_host_worker_does_not_block_shutdown() -> Result<()> {
    let (shutdown, receiver) = mpsc::channel();
    let (finished, completion) = oneshot::channel();
    let (release, gate) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        receiver.recv().expect("shutdown signal");
        gate.recv().expect("release stalled host I/O");
        let _sent = finished.send(Ok(()));
    });
    let sampler = Sampler { shutdown, finished: completion };
    let result = timeout(Duration::from_secs(1), sampler.finish(Duration::from_millis(20))).await;
    release.send(()).expect("release worker");
    worker.join().expect("worker exits");
    result.expect("bounded shutdown")?;
    Ok(())
}

#[tokio::test]
async fn shutdown_propagates_history_flush_failure() {
    let (shutdown, _) = mpsc::channel();
    let (finished, completion) = oneshot::channel();
    finished
        .send(Err(crate::error::Error::Config("flush failed".into())))
        .expect("send failure");
    let sampler = Sampler { shutdown, finished: completion };
    assert!(sampler.shutdown().await.is_err());
}

#[tokio::test]
async fn shutdown_waits_for_normal_flush_completion() -> Result<()> {
    let (shutdown, receiver) = mpsc::channel();
    let (finished, completion) = oneshot::channel();
    let worker = std::thread::spawn(move || {
        receiver.recv().expect("shutdown signal");
        let _sent = finished.send(Ok(()));
    });
    Sampler { shutdown, finished: completion }.shutdown().await?;
    worker.join().expect("worker exits");
    Ok(())
}
