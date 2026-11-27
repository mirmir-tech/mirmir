//! Removing a downloaded or downloading model. A model is every file of
//! it in any spelling: the Hub resolves a repository and a variant's label
//! in any case, a case-insensitive file system keeps one file for every
//! spelling, and an earlier version may have saved two on another. A
//! variant goes with the files no other model of its repository holds; a
//! repository with all of it.

use std::{fs, io, path::Path};

use super::{Removal, freed, variant};
use crate::{
    config::{CacheKind, HubReference, RemovalScope, RepositoryCache, Store},
    error::{Error, Result},
};

/// What a removal did, and the repository its last variant left with
/// nothing, whose own cache is to go once nothing of it transfers.
pub struct Removed {
    pub removal: Removal,
    pub emptied: Option<HubReference>,
}

/// Removes what `scope` asks for, deleting no cache it does not name.
pub async fn remove(store: &Store, scope: &RemovalScope) -> Result<Removed> {
    let asked = scope.asked().clone();
    if asked.variant().is_none() {
        let removal = remove_repository(store, scope, Caches::AnyKnown).await?;
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
pub async fn remove_emptied(
    store: &Store,
    repository: &HubReference,
    scope: &RemovalScope,
) -> Result<u64> {
    if variant::holds_any(store, repository)? {
        return Ok(0);
    }
    Ok(remove_caches(store, scope, Caches::Managed).await?.1)
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
async fn remove_repository(store: &Store, scope: &RemovalScope, caches: Caches) -> Result<Removal> {
    let reference = scope.asked();
    let damaged = [
        store.damaged_configs_of(reference)?,
        variant::damaged_markers_of(store.paths(), reference)?,
    ];
    let (mut removed, mut freed_bytes) = (false, 0);
    // Their partial blobs go with the caches.
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
            freed_bytes += variant::remove_files(store, &saved)?;
        }
        removed |= store.remove_hub_model_config(&saved.to_string())?;
    }
    // Views left by configurations that cannot be read.
    for view in variant::views_of(store, reference) {
        freed_bytes += variant::remove_view(store, reference, &view)?;
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
    let (removed_cache, cache_bytes) = remove_caches(store, scope, caches).await?;
    Ok(Removal {
        removed: removed || removed_cache,
        freed_bytes: freed_bytes + cache_bytes,
    })
}

/// Removes the repository caches `scope` names that are there: mirmir's
/// own, else, as `caches` lets it, another tool's; whether there were any,
/// and the bytes freed.
async fn remove_caches(store: &Store, scope: &RemovalScope, caches: Caches) -> Result<(bool, u64)> {
    let present = |kind| {
        let caches = scope.caches().iter();
        caches
            .filter(move |cache| cache.kind() == kind && cache.path().exists())
            .cloned()
    };
    let mut targets = present(CacheKind::Managed).collect::<Vec<_>>();
    if targets.is_empty() && caches == Caches::AnyKnown {
        targets.extend(present(CacheKind::External));
    }
    let removed = !targets.is_empty();
    let mut freed_bytes = 0;
    for cache in targets {
        freed_bytes += remove_cache_repo(store, cache).await?;
    }
    Ok((removed, freed_bytes))
}

/// Removes the repository cache `cache` and the GGUF imports that listing
/// or loading a model by a path within it made; the bytes freed. An import
/// that cannot go stays behind, unused once its files are gone.
pub(super) async fn remove_cache_repo(store: &Store, cache: RepositoryCache) -> Result<u64> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || {
        let path = validated_cache_repo(cache.path(), cache.root())?;
        let imports = store.gguf_imports_within(&path);
        let freed = freed::remove_tree(&path)?;
        Ok(freed + imports.iter().map(|import| remove_import(import)).sum::<u64>())
    })
    .await?
}

fn remove_import(import: &Path) -> u64 {
    match freed::remove_tree(import) {
        Ok(freed) => freed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => {
            tracing::warn!(import = %import.display(), %error, "GGUF import not removed");
            0
        },
    }
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
