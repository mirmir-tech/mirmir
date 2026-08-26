use super::{RuntimeService, Telemetry};
use crate::{
    application::Application,
    config::{AppConfig, Store},
};

impl RuntimeService {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let telemetry = Telemetry::new(store.paths().telemetry_file.clone());
        Self {
            application: Application::new(config, store),
            telemetry,
        }
    }
}
