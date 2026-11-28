//! What a removal may delete, decided before it starts, so that no model
//! it takes loads meanwhile and it deletes nothing else.

use std::fs;

use super::variant;
use crate::{
    catalog::discover_external_models,
    config::{
        CacheKind, HeldCache, HubReference, ModelConfig, RemovalScope, RepositoryCache, Store,
    },
    error::{Error, Result},
};

/// What removing `asked` may delete: its repository's caches in mirmir's
/// own Hub cache, in any spelling, a variant's blobs and its last one's
/// whole cache; for a repository mirmir holds nothing of, another tool's
/// cache of it, which its listing shows only then. Refused while a model
/// configured of no model of the repository, under an id of its own,
/// lies within a cache it would delete: the removal would leave it
/// configured at files that are gone.
pub fn removal_scope(store: &Store, asked: &HubReference) -> Result<RemovalScope> {
    let paths = store.paths();
    let name = asked.cache_directory();
    let mut caches = fs::read_dir(&paths.hub_cache_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_str().is_some_and(|it| it.eq_ignore_ascii_case(&name)))
        .map(|entry| RepositoryCache::new(&entry.path(), &paths.hub_cache_dir, CacheKind::Managed))
        .collect::<Vec<_>>();
    if asked.variant().is_none()
        && caches.is_empty()
        && let Some(root) = &paths.external_hub_cache
        && !holds_any(store, asked)?
    {
        let repo_id = asked.to_string();
        for model in discover_external_models(paths) {
            let cache = RepositoryCache::new(&model.repo_path, root, CacheKind::External);
            if model.repo_id.eq_ignore_ascii_case(&repo_id) && !caches.contains(&cache) {
                caches.push(cache);
            }
        }
    }
    let scope = RemovalScope::new(asked.clone(), caches);
    let held = store.list_model_configs()?.into_iter().find_map(|config| {
        let path = config.path.canonicalize().unwrap_or_else(|_| config.path.clone());
        (!of_repository(&config, asked) && scope.holds_path(&path)).then_some((config.id, path))
    });
    match held {
        Some((model, path)) => Err(Error::CacheHeld(HeldCache::new(asked.clone(), model, path))),
        None => Ok(scope),
    }
}

/// Whether `config` was saved of a model of `asked`'s repository, which
/// the removal accounts for itself.
fn of_repository(config: &ModelConfig, asked: &HubReference) -> bool {
    let saved = config.hub.as_ref().and_then(|hub| hub.repo_id.parse::<HubReference>().ok());
    saved.is_some_and(|saved| saved.same_repository(asked))
}

/// Whether mirmir holds anything of the repository `reference` that its
/// removal takes before any cache: a model, downloaded or downloading, or a
/// file that may be one but cannot be read.
fn holds_any(store: &Store, reference: &HubReference) -> Result<bool> {
    variant::holds_any(store, reference)
}
