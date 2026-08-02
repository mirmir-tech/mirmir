use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use libmir::MODEL_FORMAT_REGISTRY_SCHEMA_VERSION;

use super::RemoteHeaderPreflight;
use crate::error::{Error, Result};

const MAX_ENTRIES: usize = 32;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Key {
    repo_id: String,
    revision: String,
    schema: u32,
}

#[derive(Default)]
struct State {
    entries: HashMap<Key, RemoteHeaderPreflight>,
    order: VecDeque<Key>,
}

#[derive(Clone, Default)]
pub(in crate::catalog) struct Cache {
    state: Arc<Mutex<State>>,
}

impl Cache {
    pub fn get(&self, repo_id: &str, revision: &str) -> Result<Option<RemoteHeaderPreflight>> {
        let key = key(repo_id, revision);
        let Ok(mut state) = self.state.lock() else {
            return Err(Error::Config("preflight cache lock is poisoned".into()));
        };
        let cached = state.entries.get(&key).cloned();
        if cached.is_some() {
            state.order.retain(|candidate| candidate != &key);
            state.order.push_back(key);
        }
        drop(state);
        Ok(cached)
    }

    pub fn insert(
        &self,
        repo_id: &str,
        revision: &str,
        value: RemoteHeaderPreflight,
    ) -> Result<()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(Error::Config("preflight cache lock is poisoned".into()));
        };
        state.insert(key(repo_id, revision), value);
        drop(state);
        Ok(())
    }
}

impl State {
    fn insert(&mut self, key: Key, value: RemoteHeaderPreflight) {
        if self.entries.insert(key.clone(), value).is_some() {
            self.order.retain(|candidate| candidate != &key);
        }
        self.order.push_back(key);
        while self.entries.len() > MAX_ENTRIES {
            if let Some(expired) = self.order.pop_front() {
                self.entries.remove(&expired);
            }
        }
    }
}

fn key(repo_id: &str, revision: &str) -> Key {
    Key {
        repo_id: repo_id.to_owned(),
        revision: revision.to_owned(),
        schema: MODEL_FORMAT_REGISTRY_SCHEMA_VERSION,
    }
}

#[cfg(test)]
mod tests;
