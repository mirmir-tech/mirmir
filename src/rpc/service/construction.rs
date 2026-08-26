use super::{Activity, RuntimeService, Startup, Telemetry};
use crate::{
    application::RuntimeCoordinator,
    config::{AppConfig, Store},
};

impl RuntimeService {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let telemetry = Telemetry::new(store.paths().telemetry_file.clone());
        Self {
            coordinator: RuntimeCoordinator::new(config, store),
            telemetry,
            activity: Activity::new(),
            startup: Startup::new(),
        }
    }
}
