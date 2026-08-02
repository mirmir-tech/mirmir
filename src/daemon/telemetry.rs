use std::{
    sync::mpsc::{self, RecvTimeoutError},
    time::Duration,
};

use tokio::{sync::oneshot, time::timeout};

use crate::{error::Result, rpc::RuntimeService};

pub struct Sampler {
    shutdown: Option<mpsc::Sender<()>>,
    finished: oneshot::Receiver<Result<()>>,
}

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

impl Sampler {
    pub fn start(service: RuntimeService) -> Self {
        let (shutdown, receiver) = mpsc::channel();
        let (finished, completion) = oneshot::channel();
        let worker =
            std::thread::Builder::new().name("mirmir-telemetry".to_owned()).spawn(move || {
                let result = sample_until_shutdown(&service, &receiver);
                let _completion_result = finished.send(result);
            });
        if let Err(error) = worker {
            tracing::error!(%error, "telemetry sampler thread failed to start");
        }
        Self {
            shutdown: Some(shutdown),
            finished: completion,
        }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _shutdown_result = shutdown.send(());
        }
        match timeout(SHUTDOWN_TIMEOUT, &mut self.finished).await {
            Ok(Ok(result)) => result,
            Ok(Err(_closed)) => Ok(()),
            Err(_elapsed) => {
                tracing::warn!(
                    "telemetry sampler shutdown timed out; detaching the sampling thread"
                );
                Ok(())
            },
        }
    }
}

fn sample_until_shutdown(service: &RuntimeService, shutdown: &mpsc::Receiver<()>) -> Result<()> {
    let interval = Duration::from_millis(crate::rpc::TELEMETRY_SAMPLING_INTERVAL_MS);
    loop {
        match shutdown.recv_timeout(interval) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => record(service),
        }
    }
    service.flush_telemetry_history()?;
    Ok(())
}

fn record(service: &RuntimeService) {
    if let Err(error) = service.record_telemetry_history() {
        tracing::warn!(%error, "telemetry sample could not be recorded");
    }
}
