//! What the saved model configurations already account for, so the Hub
//! cache listing does not repeat them.

use std::{collections::HashSet, path::Path};

use super::canonical;
use crate::config::{HubReference, ModelConfig, Paths};

/// The repositories the configurations name; a downloaded GGUF variant
/// accounts for its repository's cache too.
pub(super) fn repositories(configs: &[ModelConfig]) -> HashSet<String> {
    configs
        .iter()
        .filter_map(|config| config.hub.as_ref())
        .flat_map(|hub| {
            let repository = hub
                .repo_id
                .parse::<HubReference>()
                .ok()
                .filter(|reference| reference.variant().is_some())
                .map(|reference| reference.repository().to_owned());
            std::iter::once(hub.repo_id.clone()).chain(repository)
        })
        .collect()
}

/// Whether `path` is in the Hub cache, or a GGUF variant's view of it.
pub(super) fn managed(path: &Path, paths: &Paths) -> bool {
    let path = canonical(path);
    [&paths.hub_cache_dir, &paths.gguf_variants_dir]
        .iter()
        .any(|root| path.starts_with(canonical(root)))
}
