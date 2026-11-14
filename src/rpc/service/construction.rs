use super::RuntimeService;
use crate::{
    adapters,
    application::Application,
    config::{AppConfig, Store},
    remote::RemoteAccess,
};

impl RuntimeService {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let remote = RemoteAccess::new(Store::new(store.paths().clone()));
        Self::from_application(adapters::native::application(config, store), remote)
    }

    #[must_use]
    pub const fn from_application(application: Application, remote: RemoteAccess) -> Self {
        Self { application, remote }
    }

    #[must_use]
    pub const fn remote(&self) -> &RemoteAccess {
        &self.remote
    }
}
