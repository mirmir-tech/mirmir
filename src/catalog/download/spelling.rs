//! The spelling a pull takes for a repository: the Hub resolves an owner
//! and a name in any case, so a second spelling would be a second cache
//! and configuration of one repository on a case-sensitive file system.

use super::variant::pending_variants;
use crate::{
    catalog::{discover_cached_models_in, discover_partial_downloads_in},
    config::{HubReference, Store},
    error::Result,
};

/// `reference` with its repository spelled as another model of it was
/// saved, is downloading, or is cached; else `reference` itself.
pub(in crate::catalog) fn spelled(store: &Store, reference: &HubReference) -> Result<HubReference> {
    let cache = &store.paths().hub_cache_dir;
    let cached = discover_cached_models_in(cache)
        .into_iter()
        .map(|model| model.repo_id)
        .chain(discover_partial_downloads_in(cache).into_iter().map(|model| model.repo_id))
        .filter_map(|repo_id| repo_id.parse::<HubReference>().ok());
    let saved = store
        .list_model_configs()?
        .into_iter()
        .filter_map(|model| model.hub?.repo_id.parse::<HubReference>().ok())
        .chain(pending_variants(store.paths()))
        .chain(cached)
        .find(|other| other.same_repository(reference));
    let Some(other) = saved else {
        return Ok(reference.clone());
    };
    let repository = other.repository().parse::<HubReference>()?;
    match reference.variant() {
        Some(variant) => repository.with_variant(variant),
        None => Ok(repository),
    }
}
