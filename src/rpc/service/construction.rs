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
        Self {
            library: Library::new(config.runtime.to_libmir()),
            catalog: Catalog::new(store.clone()),
            store,
            models: Arc::new(Mutex::new(HashMap::new())),
            loading: Arc::new(Mutex::new(HashSet::new())),
            telemetry,
            activity: Activity::new(),
            startup: Startup::new(),
        }
    }
}
