//! Removing a variant: its view and import, and the snapshot files and
//! blobs no other downloaded variant of the repository links.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{HubReference, Store},
    error::Result,
};

/// The directories of the variants downloaded from `reference`'s
/// repository other than its own, by the references their configurations
/// record: a repository `name` is never `owner/name`.
pub(in crate::catalog::download) fn siblings(
    store: &Store,
    reference: &HubReference,
) -> Result<Vec<PathBuf>> {
    Ok(store
        .list_model_configs()?
        .into_iter()
        .filter_map(|model| {
            let other = model.hub?.repo_id.parse::<HubReference>().ok()?;
            (other.variant().is_some()
                && other.repository() == reference.repository()
                && other != *reference)
                .then_some(model.path)
        })
        .collect())
}

/// Whether `reference`'s repository holds anything besides it: another
/// variant, downloaded or downloading, or the repository's own model.
pub(in crate::catalog::download) fn holds_more(
    store: &Store,
    reference: &HubReference,
) -> Result<bool> {
    let same = |other: &HubReference| other.repository() == reference.repository();
    if super::pending_variants(store.paths())
        .iter()
        .any(|other| same(other) && other != reference)
    {
        return Ok(true);
    }
    Ok(store.list_model_configs()?.into_iter().any(|model| {
        model
            .hub
            .and_then(|hub| hub.repo_id.parse::<HubReference>().ok())
            .is_some_and(|other| same(&other) && other != *reference)
    }))
}

/// The cache blob `snapshot_file` hard-links: the file of the same inode in
/// the repository's `blobs`.
#[cfg(unix)]
fn blob_of(snapshot_file: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let identity = |metadata: &fs::Metadata| (metadata.dev(), metadata.ino());
    let wanted = identity(&fs::metadata(snapshot_file).ok()?);
    let root = snapshot_file
        .ancestors()
        .find(|path| path.file_name() == Some("snapshots".as_ref()))?;
    let blobs = root.parent()?.join("blobs");
    fs::read_dir(blobs)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|blob| fs::metadata(blob).is_ok_and(|metadata| identity(&metadata) == wanted))
}

#[cfg(not(unix))]
fn blob_of(_snapshot_file: &Path) -> Option<PathBuf> {
    None
}

/// Removes the variant's directory and import, and the snapshot files and
/// blobs no sibling links; the bytes freed.
pub(in crate::catalog::download) fn remove_files(
    store: &Store,
    reference: &HubReference,
) -> Result<u64> {
    let view = store.paths().gguf_variants_dir.join(reference.key());
    if !view.exists() {
        return Ok(0);
    }
    let kept = siblings(store, reference)?
        .iter()
        .flat_map(|sibling| fs::read_dir(sibling).into_iter().flatten().flatten())
        .filter_map(|entry| entry.path().canonicalize().ok())
        .collect::<HashSet<_>>();
    let mut freed = 0;
    for entry in fs::read_dir(&view)?.flatten() {
        // The snapshot file the view links, a hard link of its blob.
        let Ok(snapshot_file) = entry.path().canonicalize() else {
            continue;
        };
        if kept.contains(&snapshot_file) {
            continue;
        }
        freed += fs::metadata(&snapshot_file).map_or(0, |metadata| metadata.len());
        if let Some(blob) = blob_of(&snapshot_file) {
            fs::remove_file(blob)?;
        }
        fs::remove_file(snapshot_file)?;
    }
    let view = view.canonicalize()?;
    let import = store.gguf_import_dir(&view);
    if import.exists() {
        fs::remove_dir_all(import)?;
    }
    fs::remove_dir_all(view)?;
    Ok(freed)
}
