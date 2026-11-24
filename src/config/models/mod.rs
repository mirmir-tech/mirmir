use std::{fs, path::Path};

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
        let key = if selector.contains('/') {
            model_key(selector)?
        } else {
            selector.to_owned()
        };
        let model = self.load_model_config(&key)?;
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
            models.push(self.load_model_config(key)?);
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
        write_toml(&self.model_config_path(&key), &model, false)?;
        Ok(model)
    }

    pub fn remove_hub_model_config(&self, repo_id: &str) -> Result<bool> {
        let key = model_key(repo_id)?;
        let path = self.model_config_path(&key);
        if !path.exists() {
            return Ok(false);
        }
        let model = self.load_model_config(&key)?;
        if model.hub.as_ref().is_none_or(|hub| hub.repo_id != repo_id) {
            return Err(Error::Config(format!("model `{key}` is not managed from `{repo_id}`")));
        }
        fs::remove_file(path)?;
        Ok(true)
    }

    pub(super) fn load_model_config(&self, key: &str) -> Result<ModelConfig> {
        let path = self.model_config_path(key);
        if !path.exists() {
            return Err(Error::ModelNotFound(key.to_owned()));
        }
        let model: ModelConfig = toml::from_str(&fs::read_to_string(path)?)?;
        model.validate(key)?;
        Ok(model)
    }

    pub(super) fn model_config_path(&self, key: &str) -> std::path::PathBuf {
        self.paths.models_dir.join(format!("{key}.toml"))
    }
}

/// The file-system key of a Hub reference, `owner/name[:VARIANT]`.
pub fn model_key(reference: &str) -> Result<String> {
    Ok(reference.parse::<super::HubReference>()?.key())
}

#[cfg(test)]
mod tests;
