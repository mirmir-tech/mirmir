use std::path::Path;

use super::{
    GenerationConfig, HubModelConfig, ModelConfig, Store, file::write_toml, model_key,
    model_runtime::ModelRuntimeConfig, store::config_writes,
};
use crate::error::{Error, Result};

/// The parts of a model's saved configuration one write replaces.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModelConfigChange {
    pub generation: Option<GenerationConfig>,
    pub runtime: Option<ModelRuntimeConfig>,
}

impl Store {
    /// Saves `change` into the model's configuration, creating it for a
    /// direct path. Writes are serialized with every other configuration
    /// edit, so concurrent saves of one model never drop each other's part.
    pub fn save_model_config(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        change: ModelConfigChange,
    ) -> Result<String> {
        let _writes = config_writes();
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
                    runtime: ModelRuntimeConfig::default(),
                },
                Err(error) => return Err(error),
            }
        } else {
            self.load_model_config(&super::models::selector_key(selector)?)?
        };
        if let Some(generation) = change.generation {
            model.generation = generation;
        }
        if let Some(runtime) = change.runtime {
            model.runtime = runtime;
        }
        if model.hub.is_none() {
            model.hub = hub;
        }
        self.paths.ensure_config_dirs()?;
        write_toml(&self.model_config_path(&model.id)?, &model, false)?;
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

impl GenerationConfig {
    #[must_use]
    pub const fn has_overrides(self) -> bool {
        self.max_tokens.is_some()
            || self.temperature.is_some()
            || self.top_p.is_some()
            || self.top_k.is_some()
            || self.repetition_penalty.is_some()
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
        let hub =
            HubModelConfig::new("Qwen/Test".to_owned(), "main".to_owned(), "abc123".to_owned());

        let key = store.save_model_config(
            &model_path.display().to_string(),
            "Qwen--Test",
            Some(hub),
            ModelConfigChange {
                generation: Some(generation),
                runtime: None,
            },
        )?;
        let blocks = std::num::NonZeroU32::new(2_048);
        // A context saved later keeps the generation defaults, and the reverse.
        store.save_model_config(
            &key,
            &key,
            None,
            ModelConfigChange {
                generation: None,
                runtime: Some(ModelRuntimeConfig { kv_blocks: blocks }),
            },
        )?;
        let resolved = store.resolve_model(&key)?;
        assert_eq!(resolved.generation.max_tokens, Some(512));
        assert_eq!(resolved.generation.temperature, Some(0.7));
        assert_eq!(resolved.runtime.kv_blocks, blocks);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
