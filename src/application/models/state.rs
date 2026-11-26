use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
};

use super::{ModelEntry, ModelResidency};
use crate::{
    application::{Error, Result},
    config::HubReference,
};

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
    /// The models being removed, each a model or a whole repository in any
    /// spelling, which no load starts from meanwhile.
    removing: Vec<HubReference>,
}

pub struct LoadingGuard {
    lifecycle: ModelLifecycle,
    key: String,
}

/// A removal under way: the models it names load again once it drops.
pub struct RemovalGuard {
    lifecycle: ModelLifecycle,
    asked: HubReference,
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
        if state.removing.iter().any(|asked| asked.owns_key(key)) {
            return Err(Error::ModelBeingRemoved(key.to_owned()));
        }
        if !state.loading.insert(key.to_owned()) {
            return Err(Error::ModelAlreadyLoading(key.to_owned()));
        }
        drop(state);
        Ok(LoadingGuard {
            lifecycle: self.clone(),
            key: key.to_owned(),
        })
    }

    /// Starts removing `asked`, a model or a whole repository, unless any
    /// model it names in any spelling, a variant of a repository included,
    /// is loaded or loading; none loads until the guard drops.
    pub fn begin_removal(&self, asked: &HubReference) -> Result<RemovalGuard> {
        let mut state = self.state()?;
        if let Some(key) = state.resident.keys().find(|key| asked.owns_key(key)) {
            return Err(Error::ModelLoaded(key.clone()));
        }
        if let Some(key) = state.loading.iter().find(|key| asked.owns_key(key)) {
            return Err(Error::ModelAlreadyLoading(key.clone()));
        }
        state.removing.push(asked.clone());
        drop(state);
        Ok(RemovalGuard {
            lifecycle: self.clone(),
            asked: asked.clone(),
        })
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

impl Drop for RemovalGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lifecycle.state()
            && let Some(index) = state.removing.iter().position(|asked| *asked == self.asked)
        {
            state.removing.swap_remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ModelLifecycle;
    use crate::{application::Error, config::HubReference};

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
        let _guard = lifecycle.begin_loading("Owner--Model@Q4_K_M").expect("load should start");

        // The variant in another spelling, and its repository, are it.
        for asked in ["owner/model:q4_k_m", "owner/model"] {
            let asked = asked.parse::<HubReference>().expect("a reference");
            assert!(matches!(
                lifecycle.begin_removal(&asked),
                Err(Error::ModelAlreadyLoading(model)) if model == "Owner--Model@Q4_K_M"
            ));
        }
        let sibling = "owner/model:Q8_0".parse::<HubReference>().expect("a reference");
        assert!(lifecycle.begin_removal(&sibling).is_ok());
    }

    /// No model of a repository being removed starts loading until the
    /// removal ends.
    #[test]
    fn no_load_starts_during_a_removal() {
        let lifecycle = ModelLifecycle::default();
        let asked = "Owner/Model".parse::<HubReference>().expect("a reference");
        let removal = lifecycle.begin_removal(&asked).expect("removal should start");
        assert!(matches!(
            lifecycle.begin_loading("owner--model@Q8_0"),
            Err(Error::ModelBeingRemoved(model)) if model == "owner--model@Q8_0"
        ));
        assert!(lifecycle.begin_loading("owner--other").is_ok());
        drop(removal);
        assert!(lifecycle.begin_loading("owner--model@Q8_0").is_ok());
    }
}
