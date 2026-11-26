//! What the saved model configurations already account for, so the Hub
//! cache listing does not repeat them.

use std::{collections::HashSet, path::Path};

use super::canonical;
use crate::config::{HubReference, ModelConfig, Paths};

/// Repositories and variants already listed, by id in any case: the Hub
/// resolves an id so, a case-insensitive file system holds one cache for
/// every spelling, and a removal takes them all as one.
#[derive(Debug, Default)]
pub(super) struct Repositories(HashSet<String>);

impl Repositories {
    pub(super) fn contains(&self, repo_id: &str) -> bool {
        self.0.contains(&repo_id.to_ascii_lowercase())
    }

    pub(super) fn insert(&mut self, repo_id: &str) {
        self.0.insert(repo_id.to_ascii_lowercase());
    }
}

/// The repositories the configurations name; a downloaded GGUF variant
/// accounts for its repository's cache too.
pub(super) fn repositories(configs: &[ModelConfig]) -> Repositories {
    let names = configs.iter().filter_map(|config| config.hub.as_ref()).flat_map(|hub| {
        let repository = hub
            .repo_id
            .parse::<HubReference>()
            .ok()
            .filter(|reference| reference.variant().is_some())
            .map(|reference| reference.repository().to_owned());
        std::iter::once(hub.repo_id.clone()).chain(repository)
    });
    let mut known = Repositories::default();
    for name in names {
        known.insert(&name);
    }
    known
}

/// Whether `path` is in the Hub cache, or a GGUF variant's view of it.
pub(super) fn managed(path: &Path, paths: &Paths) -> bool {
    let path = canonical(path);
    [&paths.hub_cache_dir, &paths.gguf_variants_dir]
        .iter()
        .any(|root| path.starts_with(canonical(root)))
}
