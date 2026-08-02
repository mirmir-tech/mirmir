use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use libmir::Library;

use super::{Activity, Catalog, RuntimeService, Startup, Store, Telemetry};
use crate::config::AppConfig;

impl RuntimeService {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let telemetry = Telemetry::new(store.paths().telemetry_file.clone());
        let runtime = config.runtime.to_libmir(&store.paths().state_dir);
        Self {
            library: Library::new(runtime),
            catalog: Catalog::new(store.clone()),
            store,
            models: Arc::new(Mutex::new(HashMap::new())),
            loading: Arc::new(Mutex::new(HashSet::new())),
            model_memory_gate: Arc::new(Mutex::new(())),
            model_residency: super::models::ModelResidency::default(),
            telemetry,
            activity: Activity::new(),
            startup: Startup::new(),
        }
    }
}
