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
            (other.same_repository(reference) && !other.is_same_model(reference))
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

/// What the other variants of a repository still downloading claim: their
/// blobs, by `ETag`, which one they share, such as a vision tower, is theirs
/// to resume from, and their snapshot entries, canonical, which a finished
/// blob is linked at before their view links it.
#[derive(Default)]
struct Downloading {
    blobs: HashSet<String>,
    entries: HashSet<PathBuf>,
}

fn downloading(store: &Store, reference: &HubReference) -> Downloading {
    let mut claimed = Downloading::default();
    let others = super::pending_variants(store.paths())
        .into_iter()
        .filter(|other| other.same_repository(reference) && !other.is_same_model(reference));
    for other in others {
        let Some(pending) = super::pending::recorded(store.paths(), &other) else {
            continue;
        };
        let snapshot = super::pending::snapshot(store.paths(), &pending)
            .and_then(|snapshot| snapshot.canonicalize().ok());
        for file in pending.files {
            if let Some(snapshot) = &snapshot {
                claimed.entries.insert(snapshot.join(&file.path));
            }
            claimed.blobs.insert(file.etag);
        }
    }
    claimed
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
    let downloading = downloading(store, reference).blobs;
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

/// Removes the view of the variant `reference` saved under its own key.
pub(in crate::catalog::download) fn remove_files(
    store: &Store,
    reference: &HubReference,
) -> Result<u64> {
    let view = store.paths().gguf_variants_dir.join(reference.key());
    if !view.is_dir() {
        return Ok(0);
    }
    remove_view(store, reference, &view)
}

/// The views of the model `reference` names, or of every model of a
/// repository it names, by their key in any spelling: one on a
/// case-insensitive file system, maybe more saved by an earlier version on
/// another, and views whose configuration cannot be read.
pub(in crate::catalog::download) fn views_of(
    store: &Store,
    reference: &HubReference,
) -> Vec<PathBuf> {
    let entries = fs::read_dir(&store.paths().gguf_variants_dir).into_iter().flatten().flatten();
    entries
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| reference.owns_key(name)))
        .map(|entry| entry.path())
        .collect()
}

/// Removes the variant directory `view` of `reference` and its import, and
/// the snapshot files and blobs no other model of the repository links nor
/// a downloading one records; the bytes freed.
pub(in crate::catalog::download) fn remove_view(
    store: &Store,
    reference: &HubReference,
    view: &Path,
) -> Result<u64> {
    let kept = kept_files(&siblings(store, reference)?);
    // A snapshot of another commit links the same blob at another path.
    let kept_blobs = kept.iter().filter_map(|file| identity(file)).collect::<HashSet<_>>();
    let downloading = downloading(store, reference);
    let mut freed = 0;
    for entry in fs::read_dir(view)?.flatten() {
        // The snapshot file the view links, a hard link of its blob.
        let Ok(snapshot_file) = entry.path().canonicalize() else {
            continue;
        };
        // Linked by a sibling, or claimed by a download it is to publish.
        if kept.contains(&snapshot_file) || downloading.entries.contains(&snapshot_file) {
            continue;
        }
        let blob = blob_of(&snapshot_file);
        let etag = blob.as_ref().and_then(|blob| blob.file_name()?.to_str().map(str::to_owned));
        let needed = etag.is_some_and(|etag| downloading.blobs.contains(&etag))
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
