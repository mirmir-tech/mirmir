use std::{fs, path::Path};

use super::{super::super::NativeRuntime, canonical};
use crate::{
    application::Result,
    catalog::{discover_cached_models_in, discover_external_models},
};

impl NativeRuntime {
    /// Whether a remote caller may name `selector`: empty for the default
    /// model, a loaded model, a configured key, or a path the server lists
    /// itself, that is a configured model's directory or a cached or external
    /// snapshot. Every
    /// other selector, existing or not, is refused alike, so a remote key can
    /// neither open nor probe other directories.
    pub fn offers(&self, selector: &str) -> Result<bool> {
        if selector.trim().is_empty() || self.lifecycle.state()?.resident.contains_key(selector) {
            return Ok(true);
        }
        let direct = Path::new(selector);
        if !direct.exists() {
            return Ok(self.store.resolve_model(selector).is_ok());
        }
        let Ok(target) = fs::canonicalize(direct) else {
            return Ok(false);
        };
        let paths = self.store.paths();
        let configured = self.store.list_model_configs()?.into_iter().map(|model| model.path);
        let cached = discover_cached_models_in(&paths.hub_cache_dir)
            .into_iter()
            .chain(discover_external_models(paths))
            .map(|model| model.snapshot);
        Ok(configured.chain(cached).any(|path| canonical(&path) == target))
    }
}
