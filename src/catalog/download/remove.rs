//! Removing a downloaded or downloading model. A model is every file of
//! it in any spelling: the Hub resolves a repository and a variant's label
//! in any case, a case-insensitive file system keeps one file for every
//! spelling, and an earlier version may have saved two on another. A
//! variant goes with the files no other model of its repository holds; a
//! repository with all of it.

use std::fs;

use super::{Removal, client, directory_size, variant};
use crate::{
    catalog::discover_external_models,
    config::{HubReference, Store},
    error::{Error, Result},
};

/// What a removal did, and the repository its last variant left with
/// nothing, whose own cache is to go once nothing of it transfers.
pub struct Removed {
    pub removal: Removal,
    pub emptied: Option<HubReference>,
}

pub async fn remove(store: &Store, repo_id: &str) -> Result<Removed> {
    let asked = repo_id.parse::<HubReference>()?;
    if asked.variant().is_none() {
        let removal = remove_repository(store, &asked, Caches::AnyKnown).await?;
        return Ok(Removed { removal, emptied: None });
    }
    // A variant saved under its legacy key is found by its current one.
    store.migrate_variant(&asked)?;
    // What a removal keeps is decided by the repository's other models: a
    // file that cannot be read and may be one refuses it before anything
    // goes. The variant's own unreadable files go with it.
    let damaged =
        [store.damaged_configs_of(&asked)?, variant::damaged_markers_of(store.paths(), &asked)?];
    let (mut existed, mut freed_bytes) = (false, 0);
    for pending in variant::pending_variants(store.paths()) {
        if pending.is_same_model(&asked) {
            freed_bytes += variant::remove_pending(store, &pending)?;
            variant::end_pending(store, &pending)?;
            existed = true;
        }
    }
    for view in variant::views_of(store, &asked) {
        freed_bytes += variant::remove_view(store, &asked, &view)?;
        existed = true;
    }
    for saved in saved_models(store)?.into_iter().filter(|saved| saved.is_same_model(&asked)) {
        existed |= store.remove_hub_model_config(&saved.to_string())?;
    }
    for path in damaged.into_iter().flatten() {
        fs::remove_file(path)?;
        existed = true;
    }
    // The last variant leaves the repository's own cache to its caller, to
    // take while no other of its variants transfers; a variant that never
    // existed, a base model or another variant keeps it.
    let emptied = existed && !variant::holds_more(store, &asked)?;
    let emptied = emptied.then(|| asked.repository().parse::<HubReference>()).transpose()?;
    Ok(Removed {
        removal: Removal { removed: existed, freed_bytes },
        emptied,
    })
}

/// Removes the cache a last variant left of `repository`, unless anything
/// of it was saved or began downloading since; the bytes freed.
pub async fn remove_emptied(store: &Store, repository: &HubReference) -> Result<u64> {
    if variant::holds_any(store, repository)? {
        return Ok(0);
    }
    Ok(remove_caches(store, repository, Caches::Managed).await?.1)
}

/// The Hub models saved, by the references their configurations record.
fn saved_models(store: &Store) -> Result<Vec<HubReference>> {
    Ok(store
        .list_model_configs()?
        .into_iter()
        .filter_map(|model| model.hub?.repo_id.parse::<HubReference>().ok())
        .collect())
}

/// Which caches a repository's removal may take: those it was asked for,
/// mirmir's own or another tool's it lists, or only mirmir's own.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Caches {
    AnyKnown,
    Managed,
}

/// Removes the repository `reference` with every model of it in any
/// spelling: its variants, downloaded and downloading, its own
/// configurations, their unreadable files, and its caches.
async fn remove_repository(
    store: &Store,
    reference: &HubReference,
    caches: Caches,
) -> Result<Removal> {
    let damaged = [
        store.damaged_configs_of(reference)?,
        variant::damaged_markers_of(store.paths(), reference)?,
    ];
    let mut removed = false;
    for pending in variant::pending_variants(store.paths()) {
        if pending.same_repository(reference) {
            variant::end_pending(store, &pending)?;
            removed = true;
        }
    }
    for saved in saved_models(store)?
        .into_iter()
        .filter(|saved| saved.same_repository(reference))
    {
        if saved.variant().is_some() {
            variant::remove_files(store, &saved)?;
        }
        removed |= store.remove_hub_model_config(&saved.to_string())?;
    }
    // Views left by configurations that cannot be read.
    for view in variant::views_of(store, reference) {
        variant::remove_view(store, reference, &view)?;
        removed = true;
    }
    for path in damaged.into_iter().flatten() {
        fs::remove_file(path)?;
        removed = true;
    }
    // Another tool's cache is the one asked for only when mirmir holds
    // nothing of the repository, as its listing shows that cache only then.
    let caches = if removed {
        Caches::Managed
    } else {
        caches
    };
    let (removed_cache, freed_bytes) = remove_caches(store, reference, caches).await?;
    Ok(Removal {
        removed: removed || removed_cache,
        freed_bytes,
    })
}

/// Removes the caches of the repository `reference` in any spelling, all
/// one repository to the Hub; whether there were any, and the bytes freed.
async fn remove_caches(
    store: &Store,
    reference: &HubReference,
    caches: Caches,
) -> Result<(bool, u64)> {
    let repo_id = reference.to_string();
    let same = |id: &str| id.eq_ignore_ascii_case(&repo_id);
    let own = store.paths().hub_cache_dir.clone();
    let cache = client(store)?.scan_cache().send().await?;
    let mut targets = cache
        .repos
        .into_iter()
        .filter(|repo| repo.repo_type == "model" && same(&repo.repo_id))
        .map(|repo| (repo.repo_path, own.clone()))
        .collect::<Vec<_>>();
    // Partial caches the scan does not list.
    let name = format!("models--{}", reference.key());
    for entry in fs::read_dir(&own).into_iter().flatten().flatten() {
        let path = entry.path();
        let named = entry
            .file_name()
            .to_str()
            .is_some_and(|entry| entry.eq_ignore_ascii_case(&name));
        let listed = |target: &std::path::PathBuf| {
            fs::canonicalize(target).ok() == fs::canonicalize(&path).ok()
        };
        if named && !targets.iter().any(|(target, _)| listed(target)) {
            targets.push((path, own.clone()));
        }
    }
    if targets.is_empty() && caches == Caches::AnyKnown {
        let external = store.paths().external_hub_cache.clone().and_then(|root| {
            discover_external_models(store.paths())
                .into_iter()
                .find(|model| same(&model.repo_id))
                .map(|model| (model.repo_path, root))
        });
        targets.extend(external);
    }
    let removed = !targets.is_empty();
    let mut freed_bytes = 0;
    for (path, root) in targets {
        freed_bytes += remove_cache_repo(path, root).await?;
    }
    Ok((removed, freed_bytes))
}

pub(super) async fn remove_cache_repo(
    path: std::path::PathBuf,
    cache: std::path::PathBuf,
) -> Result<u64> {
    tokio::task::spawn_blocking(move || {
        let path = validated_cache_repo(&path, &cache)?;
        let size = directory_size(&path);
        std::fs::remove_dir_all(path)?;
        Ok(size)
    })
    .await?
}

pub(super) fn validated_cache_repo(
    path: &std::path::Path,
    cache: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let cache = cache.canonicalize()?;
    let path = path.canonicalize()?;
    let is_model_repo = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("models--"));
    if path.parent() == Some(cache.as_path()) && is_model_repo {
        return Ok(path);
    }
    Err(Error::Config(format!(
        "refusing to remove model cache outside {}",
        cache.display()
    )))
}
