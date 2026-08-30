mod catalog;
mod configuration;
mod inference;
mod models;
mod ports;
mod settings;
mod telemetry;

use std::sync::Arc;

use libmir::Library;

use crate::{
    application::{Application, ModelInfo, ModelLifecycle, Result},
    catalog::Catalog,
    config::{AppConfig, StateRecovery, Store},
};

pub struct NativeRuntime {
    pub library: Library,
    pub store: Store,
    pub catalog: Catalog,
    pub lifecycle: ModelLifecycle,
}

#[must_use]
pub fn application(config: &AppConfig, store: Store) -> Application {
    let telemetry_file = store.paths().telemetry_file.clone();
    let runtime = Arc::new(NativeRuntime::new(config, store));
    Application::from_ports(runtime.clone(), runtime.clone(), runtime, telemetry_file)
}

impl NativeRuntime {
    #[must_use]
    pub fn new(config: &AppConfig, store: Store) -> Self {
        let runtime = config.runtime.to_libmir(&store.paths().state_dir);
        Self {
            library: Library::new(runtime),
            catalog: Catalog::new(store.clone()),
            store,
            lifecycle: ModelLifecycle::default(),
        }
    }

    pub fn models(&self) -> Result<Vec<ModelInfo>> {
        let state = self.lifecycle.state()?;
        let mut listed =
            state.resident.values().map(|entry| entry.info.clone()).collect::<Vec<_>>();
        drop(state);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }

    pub fn active_models(&self) -> Result<Vec<String>> {
        Ok(self.store.active_models()?)
    }

    pub fn take_state_recovery(&self) -> Result<Option<StateRecovery>> {
        Ok(self.store.take_state_recovery()?)
    }
}
