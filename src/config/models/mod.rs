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
    /// The model `selector` names, with the checkpoint it loads from: a GGUF
    /// model's import, written only when missing or older than its files.
    pub(crate) fn resolve_model(&self, selector: &str) -> Result<ResolvedModel> {
        let model = self.selected_model(selector)?;
        // A model whose files are gone keeps its path, for its load to name.
        let path = if model.path.exists() {
            self.checkpoint(&model.path)?
        } else {
            model.path
        };
        Ok(ResolvedModel { path, ..model })
    }

    /// The model `selector` names, at the path it was configured or named
    /// by, without reading its checkpoint.
    pub(crate) fn selected_model(&self, selector: &str) -> Result<ResolvedModel> {
        let direct = Path::new(selector);
        if direct.exists() {
            let canonical = fs::canonicalize(direct)?;
            if let Some(model) = self
                .list_model_configs()
                .unwrap_or_default()
                .into_iter()
                .find(|model| fs::canonicalize(&model.path).is_ok_and(|path| path == canonical))
            {
                return Ok(ResolvedModel {
                    key: model.id,
                    path: model.path,
                    generation: model.generation,
                    runtime: model.runtime,
                });
            }
            return Ok(ResolvedModel {
                key: canonical.display().to_string(),
                path: canonical,
                generation: GenerationConfig::default(),
                runtime: super::model_runtime::ModelRuntimeConfig::default(),
            });
        }
        let key = selector_key(selector)?;
        let model = match self.load_model_config(&key) {
            // A selection saved before its variant moved names its legacy
            // key, which holds no `@`.
            Err(Error::ModelNotFound(missing)) if !key.contains('@') => {
                match self.current_variant_id(&key)? {
                    Some(id) => self.load_model_config(&id)?,
                    None => return Err(Error::ModelNotFound(missing)),
                }
            },
            model => model?,
        };
        Ok(ResolvedModel {
            key: model.id,
            path: model.path,
            generation: model.generation,
            runtime: model.runtime,
        })
    }

    pub fn hub_model_downloaded(&self, repo_id: &str) -> Result<bool> {
        let key = model_key(repo_id)?;
        match self.load_model_config(&key) {
            Ok(model) => {
                Ok(model.hub.as_ref().is_some_and(|hub| hub.repo_id == repo_id)
                    && model.path.exists())
            },
            Err(Error::ModelNotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub fn list_model_configs(&self) -> Result<Vec<ModelConfig>> {
        if !self.paths.models_dir.exists() {
            return Ok(Vec::new());
        }
        // Once per listing: every configuration below is under its own key.
        self.migrate_variant_keys()?;
        let mut models = Vec::new();
        for entry in fs::read_dir(&self.paths.models_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            let Some(key) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            models.push(read_model_config(&path, key)?);
        }
        models.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(models)
    }

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
            model => model?,
        };
        if model.hub.as_ref().is_none_or(|hub| hub.repo_id != repo_id) {
            return Err(Error::Config(format!("model `{key}` is not managed from `{repo_id}`")));
        }
        fs::remove_file(self.model_config_path(&key)?)?;
        Ok(true)
    }

    pub(super) fn load_model_config(&self, key: &str) -> Result<ModelConfig> {
        let path = self.model_config_path(key)?;
        // A variant downloaded before its key took `@` is saved under its
        // legacy key, which may be this one, a namesake repository's: either
        // moves to the variant's own key first.
        if path.exists() {
            self.migrate_variant_key(&path)?;
        } else if let Some(legacy) = legacy::legacy_key(key) {
            self.migrate_variant_key(&self.model_config_path(&legacy)?)?;
        }
        if !path.exists() {
            return Err(Error::ModelNotFound(key.to_owned()));
        }
        read_model_config(&path, key)
    }

    /// Where the configuration `key` is saved; `key` is one file name.
    pub(super) fn model_config_path(&self, key: &str) -> Result<PathBuf> {
        Ok(self.paths.models_dir.join(format!("{}.toml", config_id(key)?)))
    }
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

mod legacy;
#[cfg(test)]
mod tests;
