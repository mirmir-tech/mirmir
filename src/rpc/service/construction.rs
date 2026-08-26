use super::RuntimeService;
use crate::{
    application::Application,
    config::{AppConfig, Store},
};

impl RuntimeService {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        Self {
            application: Application::new(config, store),
        }
    }
}
