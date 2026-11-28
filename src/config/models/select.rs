//! Selecting a model: by a path it is configured at or named by, or by
//! its configuration key.

use std::{fs, path::Path};

use super::{
    super::{GenerationConfig, Store, model_runtime::ModelRuntimeConfig},
    ResolvedModel, selector_key,
};
use crate::error::{Error, Result};

impl Store {
    /// The model `selector` names, ready for direct inspection or loading.
    pub(crate) fn resolve_model(&self, selector: &str) -> Result<ResolvedModel> {
        self.selected_model(selector)
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
                    vision: model.hub.and_then(|hub| hub.vision),
                    generation: model.generation,
                    runtime: model.runtime,
                });
            }
            return Ok(ResolvedModel {
                key: canonical.display().to_string(),
                path: canonical,
                vision: None,
                generation: GenerationConfig::default(),
                runtime: ModelRuntimeConfig::default(),
            });
        }
        let key = selector_key(selector)?;
        let model = match self.load_model_config(&key) {
            Err(Error::ModelNotFound(missing)) => {
                self.spelled_hub_model(&key)?.ok_or(Error::ModelNotFound(missing))?
            },
            model => model?,
        };
        Ok(ResolvedModel {
            key: model.id,
            path: model.path,
            vision: model.hub.and_then(|hub| hub.vision),
            generation: model.generation,
            runtime: model.runtime,
        })
    }
}
