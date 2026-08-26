mod error;
mod models;
mod settings;
mod telemetry;

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

pub use error::{Error, Result};
use libmir::Library;
pub use models::{LocalModelInfo, MemoryReport, ModelEntry, ModelInfo, ModelResidency};
pub use settings::{ModelInspection, ModelTaskCapabilities};

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
        assert!(coordinator.models().expect("model registry should be readable").is_empty());
    }
}
