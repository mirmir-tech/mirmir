//! Selecting a model: by a path it is configured at or named by, or by
//! its configuration key, a Hub reference's or a legacy variant's.

use std::{fs, path::Path};

use super::{
    super::{GenerationConfig, Store, model_runtime::ModelRuntimeConfig},
    Keys, ResolvedModel, selector_key,
};
use crate::error::{Error, Result};

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
        self.select(selector, Keys::Migrate)
    }

    /// The model `selector` names; with `Keys::Migrated`, without moving a
    /// legacy variant, for a caller holding a lock that the move takes.
    pub(in crate::config) fn select(&self, selector: &str, keys: Keys) -> Result<ResolvedModel> {
        let direct = Path::new(selector);
        if direct.exists() {
            let canonical = fs::canonicalize(direct)?;
            if let Some(model) = self
                .model_configs(keys)
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
                runtime: ModelRuntimeConfig::default(),
            });
        }
        let key = selector_key(selector)?;
        let model = match self.model_config(&key, keys) {
            // A selection saved before its variant moved names its legacy
            // key, which holds no `@`.
            Err(Error::ModelNotFound(missing)) if !key.contains('@') => {
                match self.current_variant_id(&key, keys)? {
                    Some(id) => self.model_config(&id, keys)?,
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
}
