//! Remote API keys: a JSON file of named, scoped digests in the state
//! directory. The file is reread when it changes, so a key created or revoked
//! while the server runs applies to the next request.

mod secret;
#[cfg(test)]
mod tests;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub use secret::display_prefix;
use serde::{Deserialize, Serialize};

use super::Scope;
use crate::{
    config::write_text,
    error::{Error, Result},
};

const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRecord {
    pub id: String,
    pub name: String,
    pub scope: Scope,
    pub created_at_unix_ms: u64,
    digest: String,
}

#[derive(Default, Serialize, Deserialize)]
struct KeyFile {
    keys: Vec<KeyRecord>,
}

pub struct KeyStore {
    path: PathBuf,
    cache: Mutex<Cached>,
}

#[derive(Default)]
struct Cached {
    modified: Option<SystemTime>,
    keys: Vec<KeyRecord>,
}

impl KeyStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(Cached::default()),
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
        let mut keys = self.list()?;
        keys.push(record.clone());
        self.save(keys)?;
        Ok((record, generated.presented))
    }

    pub fn revoke(&self, id: &str) -> Result<bool> {
        let mut keys = self.list()?;
        let before = keys.len();
        keys.retain(|key| key.id != id);
        let revoked = keys.len() != before;
        if revoked {
            self.save(keys)?;
        }
        Ok(revoked)
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
        let modified = modified(&self.path)?;
        let mut cache = self.cache.lock().map_err(|_| poisoned())?;
        if cache.modified != modified {
            cache.keys = load(&self.path, modified.is_some())?;
            cache.modified = modified;
        }
        Ok(read(&cache.keys))
    }

    fn save(&self, keys: Vec<KeyRecord>) -> Result<()> {
        let rendered = serde_json::to_string_pretty(&KeyFile { keys })?;
        write_text(&self.path, &rendered, true)?;
        *self.cache.lock().map_err(|_| poisoned())? = Cached::default();
        Ok(())
    }
}

fn load(path: &Path, exists: bool) -> Result<Vec<KeyRecord>> {
    if !exists {
        return Ok(Vec::new());
    }
    crate::config::ensure_private_file(path)?;
    let file: KeyFile = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(file.keys)
}

fn modified(path: &Path) -> Result<Option<SystemTime>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata.modified()?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(Error::Remote(format!(
            "a key name must have 1 to {MAX_NAME_CHARS} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(Error::Remote("a key name cannot contain control characters".to_owned()));
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
    Error::Remote("the key store lock is poisoned".to_owned())
}
