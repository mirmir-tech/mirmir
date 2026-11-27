use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use super::{ModelEntry, ModelResidency};
use crate::{
    application::{Error, Result},
    config::RemovalScope,
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
    /// The models loading, by key, with the canonical path each was
    /// configured or named at.
    pub loading: HashMap<String, PathBuf>,
    /// The removals under way, each of a model or a whole repository in any
    /// spelling and of the caches it may delete, none of whose models, by
    /// key or by a path within those caches, starts loading meanwhile.
    removing: Vec<RemovalScope>,
}

pub struct LoadingGuard {
    lifecycle: ModelLifecycle,
    key: String,
}

/// A removal under way: the models it names load again once it drops.
pub struct RemovalGuard {
    lifecycle: ModelLifecycle,
    scope: RemovalScope,
}

impl ModelLifecycle {
    pub fn state(&self) -> Result<MutexGuard<'_, State>> {
        self.0.state.lock().map_err(|_| Error::StatePoisoned("model lifecycle"))
    }

    pub fn memory_gate(&self) -> Result<MutexGuard<'_, ()>> {
        self.0.memory_gate.lock().map_err(|_| Error::StatePoisoned("model memory gate"))
    }

    /// Starts loading the model keyed `key`, configured or named at
    /// `source`, unless it loads already or a removal under way takes it,
    /// by its key or by its path.
    pub fn begin_loading(&self, key: &str, source: &Path) -> Result<LoadingGuard> {
        let source = source.canonicalize().unwrap_or_else(|_| source.to_path_buf());
        let mut state = self.state()?;
        if state.removing.iter().any(|scope| scope.takes(key, &source)) {
            return Err(Error::ModelBeingRemoved(key.to_owned()));
        }
        if state.loading.contains_key(key) {
            return Err(Error::ModelAlreadyLoading(key.to_owned()));
        }
        state.loading.insert(key.to_owned(), source);
        drop(state);
        Ok(LoadingGuard {
            lifecycle: self.clone(),
            key: key.to_owned(),
        })
    }

    /// Starts the removal `scope`, of a model or a whole repository, unless
    /// any model it takes is loaded or loading: one it names in any
    /// spelling, a variant of a repository included, or one configured or
    /// named at a path within a cache it may delete, whatever its key.
    /// None loads until the guard drops.
    pub fn begin_removal(&self, scope: &RemovalScope) -> Result<RemovalGuard> {
        let mut state = self.state()?;
        let resident = state.resident.iter().map(|(key, entry)| (key, &entry.source));
        if let Some((key, _)) = resident.into_iter().find(|(key, path)| scope.takes(key, path)) {
            return Err(Error::ModelLoaded(key.clone()));
        }
        if let Some((key, _)) = state.loading.iter().find(|(key, path)| scope.takes(key, path)) {
            return Err(Error::ModelAlreadyLoading(key.clone()));
        }
        state.removing.push(scope.clone());
        drop(state);
        Ok(RemovalGuard {
            lifecycle: self.clone(),
            scope: scope.clone(),
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
            && let Some(index) = state.removing.iter().position(|scope| *scope == self.scope)
        {
            state.removing.swap_remove(index);
        }
    }
}

#[cfg(test)]
mod tests;
