//! What a repository still holds once a model of it goes: whether its
//! cache has another user.

use crate::{
    config::{HubReference, Store},
    error::Result,
};

/// Whether the repository `reference` holds anything: a model, downloaded
/// or downloading, or a file that may be one but cannot be read.
pub(in crate::catalog::download) fn holds_any(
    store: &Store,
    reference: &HubReference,
) -> Result<bool> {
    let ours = |other: &HubReference| other.same_repository(reference);
    let damaged = store.damaged_configs_of(reference).map_or(true, |own| !own.is_empty())
        || super::damaged_markers_of(store.paths(), reference).map_or(true, |own| !own.is_empty());
    Ok(damaged
        || super::pending_variants(store.paths()).iter().any(ours)
        || store.list_model_configs()?.into_iter().any(|model| {
            model
                .hub
                .and_then(|hub| hub.repo_id.parse::<HubReference>().ok())
                .is_some_and(|other| ours(&other))
        }))
}

/// Whether `reference`'s repository holds anything besides it: another
/// variant, downloaded or downloading, or the repository's own model.
pub(in crate::catalog::download) fn holds_more(
    store: &Store,
    reference: &HubReference,
) -> Result<bool> {
    let other =
        |other: &HubReference| other.same_repository(reference) && !other.is_same_model(reference);
    if super::pending_variants(store.paths()).iter().any(other) {
        return Ok(true);
    }
    Ok(store.list_model_configs()?.into_iter().any(|model| {
        model
            .hub
            .and_then(|hub| hub.repo_id.parse::<HubReference>().ok())
            .is_some_and(|reference| other(&reference))
    }))
}
