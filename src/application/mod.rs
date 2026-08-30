mod activity;
mod catalog;
mod configuration;
mod error;
mod inference;
mod models;
pub mod ports;
mod restore;
mod settings;
mod startup;
mod telemetry;
mod transfer;

use std::{path::PathBuf, sync::Arc};

use activity::{Activity, Operation};
pub use activity::{
    ActivityEvent, ActivityKind, ActivityOutcome, ActivityProgress, ActivityStage, CancelOutcome,
};
pub use configuration::{ConfigurationChange, ConfigurationOutcome};
pub use error::{Error, ErrorClass, Result};
pub use inference::{GenerationEvent, GenerationResult, GenerationSession};
pub use models::{
    Check, LocalModelInfo, LocalModelState, MemoryFit, MemoryReport, ModelEntry, ModelInfo,
    eviction_can_help, eviction_candidate, rejection, safe_context, state::ModelLifecycle,
};
pub use ports::{
    CatalogPort, ConfigurationPort, ModelRuntimePort, TransferPhase, TransferProgress,
};
pub use settings::{
    EmbeddingCapabilities, ModelInspection, ModelTaskCapabilities, RerankCapabilities,
};
use startup::Startup;
pub use startup::StartupStatus;
use telemetry::Telemetry;
pub use telemetry::{
    CompletionMetrics, GenerationTelemetry, HistorySample, KvTelemetry,
    RETENTION_LIMIT as TELEMETRY_RETENTION_LIMIT, RuntimeTelemetry, SAMPLING_INTERVAL_MS,
    Snapshot as TelemetrySnapshot,
};
pub use transfer::available_message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    pub protocol_version: &'static str,
    pub server_version: &'static str,
}

pub const PROTOCOL_VERSION: &str = "1";

#[derive(Clone)]
pub struct Application {
    runtime: Arc<dyn ModelRuntimePort>,
    catalog: Arc<dyn CatalogPort>,
    configuration: Arc<dyn ConfigurationPort>,
    activity: Activity,
    startup: Startup,
    telemetry: Telemetry,
    telemetry_history: telemetry::History,
}

impl Application {
    #[must_use]
    pub fn from_ports(
        runtime: Arc<dyn ModelRuntimePort>,
        catalog: Arc<dyn CatalogPort>,
        configuration: Arc<dyn ConfigurationPort>,
        telemetry_file: PathBuf,
    ) -> Self {
        Self {
            runtime,
            catalog,
            configuration,
            activity: Activity::new(),
            startup: Startup::new(),
            telemetry: Telemetry::new(),
            telemetry_history: telemetry::History::load(telemetry_file),
        }
    }

    #[must_use]
    pub fn activity_history(&self) -> Vec<ActivityEvent> {
        self.activity.history()
    }

    #[must_use]
    pub fn activity_updates(&self) -> tokio::sync::broadcast::Receiver<ActivityEvent> {
        self.activity.subscribe()
    }

    #[must_use]
    pub fn cancel_operation(&self, operation_id: &str) -> CancelOutcome {
        self.activity.cancel(operation_id)
    }

    pub fn fail_startup(&self, detail: impl Into<String>) {
        self.startup.failed(detail);
    }

    #[must_use]
    pub const fn health() -> Health {
        Health {
            protocol_version: PROTOCOL_VERSION,
            server_version: env!("CARGO_PKG_VERSION"),
        }
    }

    #[must_use]
    pub fn startup_status(&self) -> StartupStatus {
        self.startup.status()
    }

    #[must_use]
    pub fn startup_updates(&self) -> tokio::sync::watch::Receiver<StartupStatus> {
        self.startup.subscribe()
    }

    pub fn telemetry_snapshot(&self) -> Result<TelemetrySnapshot> {
        Ok(self.telemetry.snapshot(self.runtime.telemetry()?))
    }

    pub fn record_telemetry_history(&self) -> Result<()> {
        self.telemetry_history.record(&self.telemetry_snapshot()?)?;
        Ok(())
    }

    pub fn flush_telemetry_history(&self) -> Result<()> {
        self.telemetry_history.flush()?;
        Ok(())
    }

    pub fn telemetry_history(&self, limit: u32) -> Result<Vec<HistorySample>> {
        Ok(self.telemetry_history.samples(limit)?)
    }

    pub fn models(&self) -> Result<Vec<ModelInfo>> {
        self.runtime.models()
    }

    pub fn active_models(&self) -> Result<Vec<String>> {
        self.runtime.active_models()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters,
        config::{AppConfig, Paths, Store},
    };

    #[test]
    fn exposes_transport_neutral_health_and_empty_model_registry() {
        let root = std::env::temp_dir().join("mirmir-application-contract");
        let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
        let application = adapters::native::application(&AppConfig::default(), Store::new(paths));

        assert_eq!(Application::health().protocol_version, PROTOCOL_VERSION);
        assert_eq!(Application::health().server_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(application.models().expect("model registry should be readable").len(), 0);
    }
}
