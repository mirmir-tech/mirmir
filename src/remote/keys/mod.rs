//! Remote API keys: a JSON file of named, scoped digests in the state
//! directory. The file is reread when it changes, so a key created or revoked
//! while the server runs applies to the next request.
//!
//! Every change is one transaction: the store's writer mutex and an advisory
//! lock beside the file cover rereading the file, changing it, replacing it
//! atomically and publishing the result to the verification cache, so
//! concurrent writers in this or another process never lose an update.

mod file;
mod secret;
#[cfg(test)]
mod tests;

use std::{
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use file::Stamp;
pub use secret::display_prefix;
use serde::{Deserialize, Serialize};

use super::{Scope, lock::FileLock};
use crate::error::{Error, RemoteError, Result};

const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRecord {
    pub id: String,
    pub name: String,
    pub scope: Scope,
    pub created_at_unix_ms: u64,
    digest: String,
}

pub struct KeyStore {
    path: PathBuf,
    cache: Mutex<Cached>,
    writer: Mutex<()>,
}

#[derive(Default)]
struct Cached {
    stamp: Option<Stamp>,
    keys: Vec<KeyRecord>,
}

impl KeyStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(Cached::default()),
            writer: Mutex::new(()),
        }
    }

    pub fn list(&self) -> Result<Vec<KeyRecord>> {
        self.with_current(<[KeyRecord]>::to_vec)
    }

    /// Creates a key and returns its record with the full value, which is
    /// never stored and cannot be shown again.
    pub fn create(&self, name: &str, scope: Scope) -> Result<(KeyRecord, String)> {
        let name = validate_name(name)?;
        let generated = secret::generate()?;
        let record = KeyRecord {
            id: generated.id,
            name,
            scope,
            created_at_unix_ms: now_unix_ms(),
            digest: generated.digest,
        };
        self.transact(|keys| {
            keys.push(record.clone());
            true
        })?;
        Ok((record, generated.presented))
    }

    pub fn revoke(&self, id: &str) -> Result<bool> {
        self.transact(|keys| {
            let before = keys.len();
            keys.retain(|key| key.id != id);
            keys.len() != before
        })
    }

    /// The record of a valid presented key. Malformed and unknown keys are
    /// indistinguishable to the caller.
    pub fn verify(&self, presented: &str) -> Result<Option<KeyRecord>> {
        let Some((id, secret)) = secret::parse(presented) else {
            return Ok(None);
        };
        self.with_current(|keys| {
            keys.iter()
                .find(|key| key.id == id && secret::matches(secret, &key.digest))
                .cloned()
        })
    }

    fn with_current<T>(&self, read: impl FnOnce(&[KeyRecord]) -> T) -> Result<T> {
        let stamp = file::stamp(&self.path)?;
        let mut cache = self.cache.lock().map_err(|_| poisoned())?;
        if cache.stamp != stamp {
            (cache.stamp, cache.keys) = file::load(&self.path)?;
        }
        Ok(read(&cache.keys))
    }

    /// Applies `change` to the current file contents under both writer locks,
    /// saves when it reports a change, and publishes the result to the cache
    /// before releasing the locks. Returns whether anything changed.
    fn transact(&self, change: impl FnOnce(&mut Vec<KeyRecord>) -> bool) -> Result<bool> {
        let _writer = self.writer.lock().map_err(|_| poisoned())?;
        let _file = FileLock::acquire(&self.path)?;
        let (mut stamp, mut keys) = file::load(&self.path)?;
        let changed = change(&mut keys);
        if changed {
            stamp = file::save(&self.path, &keys)?;
        }
        *self.cache.lock().map_err(|_| poisoned())? = Cached { stamp, keys };
        Ok(changed)
    }
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(Error::Remote(RemoteError::Invalid(format!(
            "a key name must have 1 to {MAX_NAME_CHARS} characters"
        ))));
    }
    if name.chars().any(char::is_control) {
        return Err(Error::Remote(RemoteError::Invalid(
            "a key name cannot contain control characters".to_owned(),
        )));
    }
    Ok(name.to_owned())
}

pub(super) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or_default()
}

fn poisoned() -> Error {
    Error::Remote(RemoteError::Failed("the key store lock is poisoned".to_owned()))
}
