use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
};

use super::{ModelEntry, ModelResidency};
use crate::application::{Error, Result};

#[derive(Clone, Default)]
pub struct ModelLifecycle(Arc<Inner>);

#[derive(Default)]
struct Inner {
    state: Mutex<State>,
    memory_gate: Mutex<()>,
    residency: ModelResidency,
}

#[derive(Default)]
pub struct State {
    pub resident: HashMap<String, ModelEntry>,
    pub loading: HashSet<String>,
}

pub struct LoadingGuard {
    lifecycle: ModelLifecycle,
    key: String,
}

impl ModelLifecycle {
    pub fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.0.state.lock().map_err(|_| Error::StatePoisoned("model lifecycle"))
    }

    pub fn memory_gate(&self) -> Result<MutexGuard<'_, ()>> {
        self.0.memory_gate.lock().map_err(|_| Error::StatePoisoned("model memory gate"))
    }

    pub fn begin_loading(&self, key: &str) -> Result<LoadingGuard> {
        let mut state = self.state()?;
        if !state.loading.insert(key.to_owned()) {
            return Err(Error::ModelAlreadyLoading(key.to_owned()));
        }
        drop(state);
        Ok(LoadingGuard {
            lifecycle: self.clone(),
            key: key.to_owned(),
        })
    }

    pub fn ensure_removable(&self, key: &str) -> Result<()> {
        let state = self.state()?;
        if state.resident.contains_key(key) {
            return Err(Error::ModelLoaded(key.to_owned()));
        }
        if state.loading.contains(key) {
            return Err(Error::ModelAlreadyLoading(key.to_owned()));
        }
        drop(state);
        Ok(())
    }

    pub fn next_residency(&self) -> u64 {
        self.0.residency.next()
    }
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lifecycle.state() {
            state.loading.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ModelLifecycle;
    use crate::application::Error;

    #[test]
    fn loading_guard_releases_the_model_on_every_exit_path() {
        let lifecycle = ModelLifecycle::default();
        let guard = lifecycle.begin_loading("model").expect("first load should start");
        assert!(matches!(
            lifecycle.begin_loading("model"),
            Err(Error::ModelAlreadyLoading(model)) if model == "model"
        ));

        drop(guard);

        assert!(lifecycle.begin_loading("model").is_ok());
    }

    #[test]
    fn removal_is_rejected_for_a_loading_model() {
        let lifecycle = ModelLifecycle::default();
        let _guard = lifecycle.begin_loading("model").expect("load should start");

        assert!(matches!(
            lifecycle.ensure_removable("model"),
            Err(Error::ModelAlreadyLoading(model)) if model == "model"
        ));
    }
}
