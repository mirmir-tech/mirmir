//! Removing a variant: its view and import, and the snapshot files and
//! blobs no other downloaded variant of the repository links.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{HubReference, Store, folder_key},
    error::Result,
};

/// The variant directories downloaded from `reference`'s repository other
/// than its own.
pub(in crate::catalog::download) fn siblings(
    store: &Store,
    reference: &HubReference,
) -> Vec<PathBuf> {
    let prefix = format!("{}--", reference.repository_key());
    let own = reference.key();
    fs::read_dir(&store.paths().gguf_variants_dir).map_or_else(
        |_| Vec::new(),
        |entries| {
            entries
                .flatten()
                .filter(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    name.starts_with(&prefix) && name != own
                })
                .map(|entry| entry.path())
                .collect()
        },
    )
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
    let kept = siblings(store, reference)
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
    let import = store.paths().gguf_imports_dir.join(folder_key(&view));
    if import.exists() {
        fs::remove_dir_all(import)?;
    }
    fs::remove_dir_all(view)?;
    Ok(freed)
}
