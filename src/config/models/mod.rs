use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{GenerationConfig, HubModelConfig, ModelConfig, Store, file::write_toml};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub key: String,
    pub path: std::path::PathBuf,
    pub generation: GenerationConfig,
    pub runtime: super::model_runtime::ModelRuntimeConfig,
}

impl Store {
    /// Records a downloaded Hub model: `reference` (`owner/name[:VARIANT]`)
    /// at `commit` of `revision`, loading from `path`.
    pub fn save_hub_model(
        &self,
        reference: &str,
        revision: &str,
        commit: &str,
        path: std::path::PathBuf,
    ) -> Result<ModelConfig> {
        let key = model_key(reference)?;
        self.paths.ensure_config_dirs()?;
        let _writes = super::store::config_writes();
        // A legacy variant saved under this key moves to its own first.
        self.migrate_variant_key(&self.model_config_path(&key)?)?;
        // A new snapshot of the repository keeps the saved defaults and context.
        let (generation, runtime) = self.load_model_config(&key).map_or_else(
            |_| (GenerationConfig::default(), super::model_runtime::ModelRuntimeConfig::default()),
            |model| (model.generation, model.runtime),
        );
        let model = ModelConfig {
            schema_version: super::schema::SCHEMA_VERSION,
            id: key.clone(),
            path,
            hub: Some(HubModelConfig {
                repo_id: reference.to_owned(),
                revision: revision.to_owned(),
                commit: commit.to_owned(),
            }),
            generation,
            runtime,
        };
        write_toml(&self.model_config_path(&key)?, &model, false)?;
        Ok(model)
    }

    pub fn remove_hub_model_config(&self, repo_id: &str) -> Result<bool> {
        let key = model_key(repo_id)?;
        let model = match self.load_model_config(&key) {
            Err(Error::ModelNotFound(_)) => return Ok(false),
            // A variant's own key holds no other model: unreadable, it goes.
            Err(error) if key.contains('@') => {
                tracing::warn!(model = %repo_id, %error, "removing a configuration that cannot be read");
                fs::remove_file(self.model_config_path(&key)?)?;
                return Ok(true);
            },
            model => model?,
        };
        if model.hub.as_ref().is_none_or(|hub| hub.repo_id != repo_id) {
            return Err(Error::Config(format!("model `{key}` is not managed from `{repo_id}`")));
        }
        fs::remove_file(self.model_config_path(&key)?)?;
        Ok(true)
    }

    pub(super) fn load_model_config(&self, key: &str) -> Result<ModelConfig> {
        self.model_config(key, Keys::Migrate)
    }

    fn model_config(&self, key: &str, keys: Keys) -> Result<ModelConfig> {
        let path = self.model_config_path(key)?;
        // A variant downloaded before its key took `@` is saved under its
        // legacy key, which may be this one, a namesake repository's: either
        // moves to the variant's own key first.
        if keys == Keys::Migrate {
            if saved(&path) {
                self.migrate_variant_key(&path)?;
            } else if let Some(legacy) = legacy::legacy_key(key) {
                self.migrate_variant_key(&self.model_config_path(&legacy)?)?;
            }
        }
        if !saved(&path) {
            return Err(Error::ModelNotFound(key.to_owned()));
        }
        read_model_config(&path, key)
    }

    /// Where the configuration `key` is saved; `key` is one file name.
    pub(super) fn model_config_path(&self, key: &str) -> Result<PathBuf> {
        Ok(self.paths.models_dir.join(format!("{}.toml", config_id(key)?)))
    }
}

/// Whether a lookup moves legacy variant keys first, or its caller moved
/// them and holds the UX state lock, which moving takes again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::config) enum Keys {
    Migrate,
    Migrated,
}

/// Whether a configuration is saved at `path` under that very name: on a
/// case-insensitive file system, a key's file answers to its other
/// spellings too, which are other keys.
fn saved(path: &Path) -> bool {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return false;
    };
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.file_name() == name)
}

fn read_model_config(path: &Path, key: &str) -> Result<ModelConfig> {
    let model: ModelConfig = toml::from_str(&fs::read_to_string(path)?)?;
    model.validate(key)?;
    Ok(model)
}

/// `id` when it can name a configuration file: a key's characters, ASCII
/// letters, digits, `-`, `_`, `.` and `@`, never `.` or `..`.
fn config_id(id: &str) -> Result<&str> {
    let valid = !id.is_empty()
        && id != "."
        && id != ".."
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.@".contains(character));
    if valid {
        Ok(id)
    } else {
        Err(Error::Config(format!("invalid model configuration id `{id}`")))
    }
}

/// The file-system key of a Hub reference, `owner/name[:VARIANT]`.
pub fn model_key(reference: &str) -> Result<String> {
    Ok(reference.parse::<super::HubReference>()?.key())
}

/// The key a configuration `selector` names: a Hub reference's, with or
/// without an owner, else the selector itself, a configuration id. A path
/// is selected only as one that exists, never as an id.
pub(super) fn selector_key(selector: &str) -> Result<String> {
    model_key(selector).or_else(|_| Ok(config_id(selector)?.to_owned()))
}

mod installed;
mod legacy;
mod listing;
mod select;
#[cfg(test)]
mod tests;
