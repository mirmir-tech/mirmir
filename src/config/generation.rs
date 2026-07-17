use std::path::Path;

use super::{GenerationConfig, HubModelConfig, ModelConfig, Store, file::write_toml, model_key};
use crate::error::{Error, Result};

impl Store {
    pub fn save_model_generation(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        generation: GenerationConfig,
    ) -> Result<String> {
        let direct = Path::new(selector);
        let mut model = if direct.exists() {
            validate_id(config_id, hub.as_ref())?;
            match self.load_model_config(config_id) {
                Ok(model) => model,
                Err(Error::ModelNotFound(_)) => ModelConfig {
                    schema_version: super::schema::SCHEMA_VERSION,
                    id: config_id.to_owned(),
                    path: direct.to_owned(),
                    hub: hub.clone(),
                    generation: GenerationConfig::default(),
                },
                Err(error) => return Err(error),
            }
        } else {
            let key = if selector.contains('/') {
                model_key(selector)?
            } else {
                selector.to_owned()
            };
            self.load_model_config(&key)?
        };
        model.generation = generation;
        if model.hub.is_none() {
            model.hub = hub;
        }
        self.paths.ensure_config_dirs()?;
        write_toml(&self.model_config_path(&model.id), &model, false)?;
        Ok(model.id)
    }
}

fn validate_id(id: &str, hub: Option<&HubModelConfig>) -> Result<()> {
    if id.is_empty() {
        return Err(Error::Config("model configuration id cannot be empty".to_owned()));
    }
    let expected = hub.map_or_else(|| model_key(id), |hub| model_key(&hub.repo_id))?;
    if expected == id {
        Ok(())
    } else {
        Err(Error::Config(format!("model id `{id}` does not match `{expected}`")))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn persists_external_model_generation_overrides() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "mirmir-generation-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("settings")
        ));
        let model_path = root.join("cache/snapshots/abc123");
        fs::create_dir_all(&model_path)?;
        let store = Store::new(super::super::Paths::from_roots(
            root.join("config"),
            root.join("state"),
            &root.join("runtime"),
        ));
        let generation = GenerationConfig {
            max_tokens: Some(512),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            repetition_penalty: Some(1.1),
        };
        let hub = HubModelConfig {
            repo_id: "Qwen/Test".to_owned(),
            revision: "main".to_owned(),
            commit: "abc123".to_owned(),
        };

        let key = store.save_model_generation(
            &model_path.display().to_string(),
            "Qwen--Test",
            Some(hub),
            generation,
        )?;
        let resolved = store.resolve_model(&key)?;
        assert_eq!(resolved.generation.max_tokens, Some(512));
        assert_eq!(resolved.generation.temperature, Some(0.7));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
