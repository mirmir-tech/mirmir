use std::{fs, path::Path};

use super::{GenerationConfig, HubModelConfig, ModelConfig, Store, file::write_toml};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub key: String,
    pub path: std::path::PathBuf,
    pub generation: GenerationConfig,
}

impl Store {
    pub(crate) fn resolve_model(&self, selector: &str) -> Result<ResolvedModel> {
        let direct = Path::new(selector);
        if direct.exists() {
            return Ok(ResolvedModel {
                key: direct.display().to_string(),
                path: direct.to_owned(),
                generation: GenerationConfig::default(),
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

    pub fn save_hub_model(
        &self,
        repo_id: &str,
        revision: &str,
        snapshot: std::path::PathBuf,
    ) -> Result<ModelConfig> {
        let key = model_key(repo_id)?;
        self.paths.ensure_config_dirs()?;
        let generation = self
            .load_model_config(&key)
            .map_or_else(|_| GenerationConfig::default(), |model| model.generation);
        let commit = snapshot
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::Config("Hub snapshot has no commit directory".to_owned()))?
            .to_owned();
        let model = ModelConfig {
            schema_version: super::schema::SCHEMA_VERSION,
            id: key.clone(),
            path: snapshot,
            hub: Some(HubModelConfig {
                repo_id: repo_id.to_owned(),
                revision: revision.to_owned(),
                commit,
            }),
            generation,
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

pub fn model_key(repo_id: &str) -> Result<String> {
    let parts = repo_id.split('/').collect::<Vec<_>>();
    let valid_part = |part: &&str| {
        !part.is_empty()
            && *part != "."
            && *part != ".."
            && part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    };
    if !(1..=2).contains(&parts.len()) || !parts.iter().all(valid_part) {
        return Err(Error::Config(format!("invalid Hugging Face repository id `{repo_id}`")));
    }
    Ok(parts.join("--"))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_STORE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn creates_stable_key_and_rejects_traversal() -> Result<()> {
        assert_eq!(model_key("Qwen/Qwen2.5")?, "Qwen--Qwen2.5");
        assert!(model_key("../secret").is_err());
        assert!(model_key("owner/model/extra").is_err());
        Ok(())
    }

    #[test]
    fn persists_and_removes_hub_model_record() -> Result<()> {
        let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("mirmir-hub-model-{}-{unique}", std::process::id()));
        let store = Store::new(super::super::Paths::from_roots(
            root.join("config"),
            root.join("state"),
            &root.join("runtime"),
        ));
        let snapshot = root.join("cache/snapshots/0123456789");
        fs::create_dir_all(&snapshot)?;
        let model = store.save_hub_model("Qwen/Test", "main", snapshot)?;
        assert_eq!(model.hub.as_ref().map(|hub| hub.commit.as_str()), Some("0123456789"));
        assert!(store.hub_model_downloaded("Qwen/Test")?);
        assert_eq!(store.resolve_model("Qwen/Test")?.key, "Qwen--Test");
        assert!(store.remove_hub_model_config("Qwen/Test")?);
        assert!(!store.hub_model_downloaded("Qwen/Test")?);
        Ok(())
    }
}
