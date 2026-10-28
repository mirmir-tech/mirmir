mod http;

use std::{
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    time::Duration,
};

use axum::{http::StatusCode, response::IntoResponse};
use tokio::sync::oneshot;

use super::worker::Worker;

const WAIT: Duration = Duration::from_secs(5);

type Outcome<T> = Result<T, Box<dyn std::error::Error>>;

/// A worker that reports each job it starts, holds it until `gate` releases
/// it, then reports whether its requester had gone by then.
struct Gated {
    worker: Worker<u32, u32>,
    started: Receiver<u32>,
    finished: Receiver<(u32, bool)>,
    gate: Sender<()>,
}

fn gated(capacity: usize) -> Gated {
    let (start, started) = channel();
    let (finish, finished) = channel();
    let (gate, release) = channel::<()>();
    let worker = Worker::spawn(capacity, move || {
        move |job, cancelled: &dyn Fn() -> bool| {
            let _observer_gone = start.send(job);
            let _released = release.recv_timeout(WAIT);
            let _observer_gone = finish.send((job, cancelled()));
            job
        }
    });
    Gated { worker, started, finished, gate }
}

impl Gated {
    fn submit(&self, job: u32) -> Outcome<oneshot::Receiver<u32>> {
        self.worker.submit(job).map_err(|error| format!("{error:?}").into())
    }

    fn release(&self, jobs: usize) -> Outcome<()> {
        for _ in 0..jobs {
            self.gate.send(())?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn skips_queued_jobs_whose_requester_is_gone() -> Outcome<()> {
    let gated = gated(4);
    let first = gated.submit(1)?;
    assert_eq!(gated.started.recv_timeout(WAIT)?, 1);
    let abandoned = gated.submit(2)?;
    let last = gated.submit(3)?;
    drop(abandoned);
    gated.release(2)?;
    assert_eq!(first.await?, 1);
    assert_eq!(last.await?, 3);
    assert_eq!(gated.started.recv_timeout(WAIT)?, 3);
    assert_eq!(gated.finished.recv_timeout(WAIT)?, (1, false));
    assert_eq!(gated.finished.recv_timeout(WAIT)?, (3, false));
    Ok(())
}

#[tokio::test]
async fn a_running_job_sees_its_requester_leave() -> Outcome<()> {
    let gated = gated(1);
    let running = gated.submit(1)?;
    assert_eq!(gated.started.recv_timeout(WAIT)?, 1);
    drop(running);
    gated.release(1)?;
    assert_eq!(gated.finished.recv_timeout(WAIT)?, (1, true));
    Ok(())
}

#[tokio::test]
async fn refuses_jobs_beyond_the_queue_capacity() -> Outcome<()> {
    let gated = gated(1);
    let running = gated.submit(1)?;
    assert_eq!(gated.started.recv_timeout(WAIT)?, 1);
    let queued = gated.submit(2)?;
    let refused = gated.worker.submit(3).err().ok_or("a full queue must refuse the job")?;
    assert_eq!(refused.into_response().status(), StatusCode::SERVICE_UNAVAILABLE);
    gated.release(2)?;
    assert_eq!(running.await?, 1);
    assert_eq!(queued.await?, 2);
    Ok(())
}

#[test]
fn the_thread_exits_once_every_handle_is_dropped() {
    let (alive, exited) = channel::<()>();
    let worker = Worker::<u32, u32>::spawn(1, move || {
        move |job, _: &dyn Fn() -> bool| {
            let _held = &alive;
            job
        }
    });
    let clone = worker.clone();
    drop(worker);
    assert_eq!(exited.recv_timeout(Duration::from_millis(100)), Err(RecvTimeoutError::Timeout));
    drop(clone);
    assert_eq!(exited.recv_timeout(WAIT), Err(RecvTimeoutError::Disconnected));
}

#[test]
#[ignore = "needs a Laya checkpoint in MIRMIR_LAYA_MODEL"]
fn an_unreadable_checkpoint_keeps_the_resident_model() -> Outcome<()> {
    let checkpoint = std::path::PathBuf::from(std::env::var("MIRMIR_LAYA_MODEL")?);
    let request = |checkpoint: std::path::PathBuf| super::DecideRequest {
        checkpoint,
        backend: super::BackendName::Cpu,
        state: super::types::StateInput::Text("We were billed twice for March.".into()),
        schema: serde_json::json!({"type": "object", "properties": {"refund": {"type": "boolean"}}}),
    };
    let mut loaded: super::Loaded = None;
    let live = || false;
    super::answer(&mut loaded, request(checkpoint.clone()), &live)
        .map_err(|error| format!("{error:?}"))?;
    let resident = loaded.as_ref().map(|(key, _)| key.clone());
    assert_eq!(resident, Some((checkpoint, super::BackendName::Cpu)));
    let missing = super::answer(&mut loaded, request("/nonexistent/laya".into()), &live);
    assert!(missing.is_err());
    assert_eq!(loaded.as_ref().map(|(key, _)| key.clone()), resident);
    Ok(())
}
