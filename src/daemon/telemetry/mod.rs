use std::{
    sync::mpsc::{self, RecvTimeoutError},
    time::Duration,
};

use tokio::{sync::oneshot, time::timeout};

use crate::{application::TelemetryRecorder, error::Result, rpc::RuntimeService};

pub struct Sampler {
    shutdown: mpsc::Sender<()>,
    finished: oneshot::Receiver<Result<()>>,
}

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

impl Sampler {
    pub fn start(service: &RuntimeService) -> Self {
        // Extract host-only observers before spawning. No runtime, model or
        // accelerator handle may be captured by this potentially blocking worker.
        let recorder = service.telemetry_recorder();
        let (shutdown, receiver) = mpsc::channel();
        let (finished, completion) = oneshot::channel();
        let _worker = std::thread::Builder::new()
            .name("mirmir-telemetry".to_owned())
            .spawn(move || {
                let result = sample_until_shutdown(&recorder, &receiver);
                drop(recorder);
                let _sent = finished.send(result);
            })
            .inspect_err(
                |error| tracing::error!(%error, "telemetry sampler thread failed to start"),
            );
        Self { shutdown, finished: completion }
    }

    pub async fn shutdown(self) -> Result<()> {
        self.finish(SHUTDOWN_TIMEOUT).await
    }

    async fn finish(self, deadline: Duration) -> Result<()> {
        let _sent = self.shutdown.send(());
        if let Ok(result) = timeout(deadline, self.finished).await {
            result?
        } else {
            // A stalled OS read or history write owns only host-side data.
            // No blocking join keeps Tokio alive and no inference runtime is retained.
            tracing::warn!("telemetry shutdown timed out; final history flush may be incomplete");
            Ok(())
        }
    }
}

fn sample_until_shutdown(
    recorder: &TelemetryRecorder,
    shutdown: &mpsc::Receiver<()>,
) -> Result<()> {
    let interval = Duration::from_millis(crate::rpc::TELEMETRY_SAMPLING_INTERVAL_MS);
    loop {
        match shutdown.recv_timeout(interval) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if let Err(error) = recorder.record() {
                    tracing::warn!(%error, "telemetry sample could not be recorded");
                }
            },
        }
    }
    recorder.flush()
}

#[cfg(test)]
mod tests;
