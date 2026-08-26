mod activity;
mod catalog;
mod configuration;
mod error;
mod inference;
mod models;
mod restore;
mod settings;
mod startup;
mod telemetry;
mod transfer;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

pub use activity::{Activity, ActivityEvent, CancelOutcome, Operation};
pub use configuration::ConfigurationChange;
pub use error::{Error, Result};
pub use inference::{GenerationEvent, GenerationResult, GenerationSession};
use libmir::Library;
pub use models::{LocalModelInfo, MemoryReport, ModelEntry, ModelInfo, ModelResidency};
pub use settings::{
    EmbeddingCapabilities, ModelInspection, ModelTaskCapabilities, RerankCapabilities,
};
pub use startup::{Snapshot as StartupSnapshot, Startup};
pub use telemetry::{
    CompletionMetrics, GenerationTelemetry, HistorySample,
    RETENTION_LIMIT as TELEMETRY_RETENTION_LIMIT, SAMPLING_INTERVAL_MS,
    Snapshot as TelemetrySnapshot, Telemetry,
};
pub use transfer::available_message;

use crate::{
    catalog::Catalog,
    config::{AppConfig, Store},
};

#[derive(Clone)]
pub struct RuntimeCoordinator {
    pub(crate) library: Library,
    pub(crate) store: Store,
    pub(crate) catalog: Catalog,
    pub(crate) models: Arc<Mutex<HashMap<String, ModelEntry>>>,
    pub(crate) loading: Arc<Mutex<HashSet<String>>>,
    pub(crate) model_memory_gate: Arc<Mutex<()>>,
    pub(crate) model_residency: ModelResidency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    pub protocol_version: &'static str,
    pub server_version: &'static str,
}

pub const PROTOCOL_VERSION: &str = "1";

#[derive(Clone)]
pub struct Application {
    pub(crate) runtime: RuntimeCoordinator,
    pub(crate) activity: Activity,
    pub(crate) startup: Startup,
    pub(crate) telemetry: Telemetry,
    telemetry_history: telemetry::History,
}

impl Application {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let telemetry_history = telemetry::History::load(store.paths().telemetry_file.clone());
        Self {
            runtime: RuntimeCoordinator::new(config, store),
            activity: Activity::new(),
            startup: Startup::new(),
            telemetry: Telemetry::new(),
            telemetry_history,
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
    pub fn startup_snapshot(&self) -> StartupSnapshot {
        self.startup.snapshot()
    }

    #[must_use]
    pub fn startup_updates(&self) -> tokio::sync::watch::Receiver<StartupSnapshot> {
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
}

impl RuntimeCoordinator {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let runtime = config.runtime.to_libmir(&store.paths().state_dir);
        Self {
            library: Library::new(runtime),
            catalog: Catalog::new(store.clone()),
            store,
            models: Arc::new(Mutex::new(HashMap::new())),
            loading: Arc::new(Mutex::new(HashSet::new())),
            model_memory_gate: Arc::new(Mutex::new(())),
            model_residency: ModelResidency::default(),
        }
    }

    #[must_use]
    pub const fn health() -> Health {
        Health {
            protocol_version: PROTOCOL_VERSION,
            server_version: env!("CARGO_PKG_VERSION"),
        }
    }

    pub fn models(&self) -> Result<Vec<ModelInfo>> {
        let Ok(models) = self.models.lock() else {
            return Err(Error::StatePoisoned("model registry"));
        };
        let mut listed = models.values().map(|entry| entry.info.clone()).collect::<Vec<_>>();
        drop(models);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }

    pub fn active_models(&self) -> Result<Vec<String>> {
        Ok(self.store.active_models()?)
    }

    pub fn take_state_recovery(&self) -> Result<Option<crate::config::StateRecovery>> {
        Ok(self.store.take_state_recovery()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;

    #[test]
    fn exposes_transport_neutral_health_and_empty_model_registry() {
        let root = std::env::temp_dir().join("mirmir-application-contract");
        let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
        let coordinator = RuntimeCoordinator::new(&AppConfig::default(), Store::new(paths));

        assert_eq!(RuntimeCoordinator::health().protocol_version, PROTOCOL_VERSION);
        assert_eq!(RuntimeCoordinator::health().server_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(coordinator.models().expect("model registry should be readable").len(), 0);
    }
}
