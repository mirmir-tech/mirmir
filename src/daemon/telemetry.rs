use std::time::Duration;

use tokio::{sync::oneshot, task::JoinHandle, time::MissedTickBehavior};

use crate::{error::Result, rpc::RuntimeService};

pub struct Sampler {
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    service: RuntimeService,
}

impl Sampler {
    pub fn start(service: RuntimeService) -> Self {
        let (shutdown, mut receiver) = oneshot::channel();
        let sampled = service.clone();
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(
                crate::rpc::TELEMETRY_SAMPLING_INTERVAL_MS,
            ));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = interval.tick() => record(sampled.clone()).await,
                    _result = &mut receiver => break,
                }
            }
        });
        Self { shutdown: Some(shutdown), task, service }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _shutdown_result = shutdown.send(());
        }
        self.task.await?;
        tokio::task::spawn_blocking(move || self.service.flush_telemetry_history()).await??;
        Ok(())
    }
}

async fn record(service: RuntimeService) {
    let result = tokio::task::spawn_blocking(move || service.record_telemetry_history()).await;
    match result {
        Ok(Ok(())) => {},
        Ok(Err(error)) => tracing::warn!(%error, "telemetry sample could not be recorded"),
        Err(error) => tracing::warn!(%error, "telemetry sampler task failed"),
    }
}
