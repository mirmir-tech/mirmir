//! The single decision worker thread and its bounded queue.
//!
//! A job whose requester has gone (disconnect, request timeout, shutdown) is
//! skipped before it starts, and the running job can poll the same signal
//! between stages. A stage already running finishes and its result is
//! dropped. The thread exits once every handle is dropped and the job it is
//! running, if any, returns.

use std::{
    sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    thread,
};

use tokio::sync::oneshot;

use crate::http::error::ApiError;

type Job<J, T> = (J, oneshot::Sender<T>);

pub struct Worker<J, T> {
    jobs: SyncSender<Job<J, T>>,
}

impl<J, T> Clone for Worker<J, T> {
    fn clone(&self) -> Self {
        Self { jobs: self.jobs.clone() }
    }
}

impl<J: Send + 'static, T: Send + 'static> Worker<J, T> {
    /// Admits up to `capacity` jobs behind the running one. `make` runs on
    /// the worker thread, so the state it builds never crosses threads.
    pub fn spawn<W>(capacity: usize, make: impl FnOnce() -> W + Send + 'static) -> Self
    where
        W: FnMut(J, &dyn Fn() -> bool) -> T,
    {
        let (jobs, requests) = sync_channel(capacity);
        let _detached = thread::spawn(move || run(&requests, make()));
        Self { jobs }
    }

    /// Queues `job`, or refuses it at once when the queue is full.
    pub fn submit(&self, job: J) -> Result<oneshot::Receiver<T>, ApiError> {
        let (reply, response) = oneshot::channel();
        match self.jobs.try_send((job, reply)) {
            Ok(()) => Ok(response),
            Err(TrySendError::Full(_)) => {
                Err(ApiError::unavailable("the decision queue is full; retry later"))
            },
            Err(TrySendError::Disconnected(_)) => {
                Err(ApiError::internal("the decision worker stopped"))
            },
        }
    }
}

fn run<J, T>(requests: &Receiver<Job<J, T>>, mut work: impl FnMut(J, &dyn Fn() -> bool) -> T) {
    for (job, reply) in requests {
        if reply.is_closed() {
            continue;
        }
        let result = work(job, &|| reply.is_closed());
        let _requester_gone = reply.send(result);
    }
}
