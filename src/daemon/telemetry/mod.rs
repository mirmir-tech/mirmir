use std::{
    sync::mpsc::{self, RecvTimeoutError},
    thread::JoinHandle,
    time::Duration,
};

use tokio::{task::spawn_blocking, time::timeout};

use crate::{error::Result, rpc::RuntimeService};

pub struct Sampler {
    shutdown: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<Result<()>>>,
}

/// After this long, shutdown reports that it is still waiting for the
/// sampling thread. It keeps waiting: the thread holds runtime state, and
/// the process must not exit while another thread is still releasing it.
const SHUTDOWN_WARNING: Duration = Duration::from_secs(2);

impl Sampler {
    pub fn start(service: RuntimeService) -> Self {
        let (shutdown, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("mirmir-telemetry".to_owned())
            .spawn(move || sample_until_shutdown(&service, &receiver))
            .inspect_err(
                |error| tracing::error!(%error, "telemetry sampler thread failed to start"),
            )
            .ok();
        Self { shutdown: Some(shutdown), worker }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _shutdown_result = shutdown.send(());
        }
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        // Joining also waits for the thread to drop its runtime handle.
        let mut joined = spawn_blocking(move || {
            worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        });
        if let Ok(result) = timeout(SHUTDOWN_WARNING, &mut joined).await {
            return result?;
        }
        tracing::warn!("telemetry sampler is still finishing; waiting for it before exit");
        joined.await?
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

#[cfg(test)]
mod tests;
