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

/// The directories of the models saved from `reference`'s repository other
/// than itself, by the references their configurations record: its other
/// variants' views and the repository's own snapshot, whose files a variant
/// may share. A repository `name` is never `owner/name`.
pub(in crate::catalog::download) fn siblings(
    store: &Store,
    reference: &HubReference,
) -> Result<Vec<PathBuf>> {
    Ok(store
        .list_model_configs()?
        .into_iter()
        .filter_map(|model| {
            let other = model.hub?.repo_id.parse::<HubReference>().ok()?;
            (other.repository() == reference.repository() && other != *reference)
                .then_some(model.path)
        })
        .collect())
}

/// The files under the `siblings`, canonical: a view's links resolve to
/// the snapshot files they name, a snapshot's own files to themselves.
fn kept_files(siblings: &[PathBuf]) -> HashSet<PathBuf> {
    let mut kept = HashSet::new();
    let mut directories = siblings.to_vec();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).into_iter().flatten().flatten() {
            let path = entry.path();
            // A snapshot nests a repository's folders; a view is flat.
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                directories.push(path);
            } else if let Ok(file) = path.canonicalize() {
                kept.insert(file);
            }
        }
    }
    kept
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

/// The file `path` names, as its device and inode: every hard link of a
/// blob, in any commit's snapshot, has the blob's.
#[cfg(unix)]
fn identity(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).ok().map(|metadata| (metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn identity(_path: &Path) -> Option<(u64, u64)> {
    None
}

/// The cache blob `snapshot_file` hard-links: the file of the same inode in
/// the repository's `blobs`.
fn blob_of(snapshot_file: &Path) -> Option<PathBuf> {
    let wanted = identity(snapshot_file)?;
    let root = snapshot_file
        .ancestors()
        .find(|path| path.file_name() == Some("snapshots".as_ref()))?;
    let blobs = root.parent()?.join("blobs");
    fs::read_dir(blobs)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|blob| identity(blob) == Some(wanted))
}

/// The blobs, by `ETag`, of the other variants of `reference`'s repository
/// still downloading: one they share with it, such as a vision tower, is
/// theirs to resume from.
fn pending_blobs(store: &Store, reference: &HubReference) -> HashSet<String> {
    super::pending_variants(store.paths())
        .into_iter()
        .filter(|other| other != reference && other.repository() == reference.repository())
        .filter_map(|other| super::pending::recorded(store.paths(), &other))
        .flat_map(|other| other.files.into_iter().map(|file| file.etag))
        .collect()
}

/// Removes the files a downloading `reference` recorded, final or partial,
/// except blobs a sibling variant links or another downloading one records;
/// the bytes freed.
pub(in crate::catalog::download) fn remove_pending(
    store: &Store,
    reference: &HubReference,
) -> Result<u64> {
    let Some(pending) = super::pending::recorded(store.paths(), reference) else {
        return Ok(0);
    };
    let kept = kept_files(&siblings(store, reference)?)
        .iter()
        .filter_map(|file| blob_of(file))
        .collect::<HashSet<_>>();
    let root = store
        .paths()
        .hub_cache_dir
        .join(format!("models--{}", crate::config::model_key(reference.repository())?));
    let snapshot = root.join("snapshots").join(&pending.commit);
    let downloading = pending_blobs(store, reference);
    let mut freed = 0;
    for ((blob, partial), file) in pending.blobs(store.paths())?.into_iter().zip(&pending.files) {
        if downloading.contains(&file.etag)
            || blob.canonicalize().is_ok_and(|blob| kept.contains(&blob))
        {
            continue;
        }
        for path in [partial, blob] {
            if let Ok(metadata) = fs::metadata(&path) {
                freed += metadata.len();
                fs::remove_file(path)?;
            }
        }
        // The snapshot's hard link of a finished blob.
        match fs::remove_file(snapshot.join(&file.path)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {},
        }
    }
    Ok(freed)
}

/// Removes the variant's directory and import, and the snapshot files and
/// blobs no sibling links nor a downloading variant records; the bytes
/// freed.
pub(in crate::catalog::download) fn remove_files(
    store: &Store,
    reference: &HubReference,
) -> Result<u64> {
    let view = store.paths().gguf_variants_dir.join(reference.key());
    if !view.exists() {
        return Ok(0);
    }
    let kept = kept_files(&siblings(store, reference)?);
    // A snapshot of another commit links the same blob at another path.
    let kept_blobs = kept.iter().filter_map(|file| identity(file)).collect::<HashSet<_>>();
    let downloading = pending_blobs(store, reference);
    let mut freed = 0;
    for entry in fs::read_dir(&view)?.flatten() {
        // The snapshot file the view links, a hard link of its blob.
        let Ok(snapshot_file) = entry.path().canonicalize() else {
            continue;
        };
        if kept.contains(&snapshot_file) {
            continue;
        }
        let blob = blob_of(&snapshot_file);
        let etag = blob.as_ref().and_then(|blob| blob.file_name()?.to_str().map(str::to_owned));
        let needed = etag.is_some_and(|etag| downloading.contains(&etag))
            || identity(&snapshot_file).is_some_and(|file| kept_blobs.contains(&file));
        if let Some(blob) = blob.filter(|_| !needed) {
            freed += fs::metadata(&blob).map_or(0, |metadata| metadata.len());
            fs::remove_file(blob)?;
        }
        // Its blob kept, the entry only names it; else its bytes go with it.
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
