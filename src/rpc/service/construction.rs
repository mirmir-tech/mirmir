use super::RuntimeService;
use crate::{
    adapters,
    application::Application,
    config::{AppConfig, Store},
};

impl RuntimeService {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        Self::from_application(adapters::native::application(config, store))
    }

    #[must_use]
    pub const fn from_application(application: Application) -> Self {
        Self { application }
    }
}
