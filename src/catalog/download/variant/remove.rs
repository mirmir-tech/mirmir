//! Removing a variant: its view and import, and the snapshot files and
//! blobs no other downloaded variant of the repository links.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use super::{
    super::freed,
    blobs::{Blob, Blobs, Identity, Naming, identity},
    snapshot::{self, Key},
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

/// What the `siblings` hold: their files canonical, as a view's links
/// resolve to the snapshot entries they name and the blobs those name; the
/// snapshot entries they are or link; and their files' identities, which a
/// blob hard-linked or linked at another commit shares.
#[derive(Default)]
struct Kept {
    files: HashSet<PathBuf>,
    entries: HashSet<Key>,
    blobs: HashSet<Identity>,
}

fn kept(siblings: &[PathBuf]) -> Kept {
    let mut kept = Kept::default();
    let mut directories = siblings.to_vec();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory).into_iter().flatten().flatten() {
            let path = entry.path();
            // A snapshot nests a repository's folders; a view is flat.
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                directories.push(path);
            } else {
                kept.files.extend(path.canonicalize().ok());
                kept.entries.extend(snapshot::entry(&path).as_deref().and_then(snapshot::key));
                kept.blobs.extend(identity(&path));
            }
        }
    }
    kept
}

/// What the other variants of a repository still downloading claim: their
/// blobs, by `ETag`, which one they share, such as a vision tower, is theirs
/// to resume from, and their snapshot entries, canonical, which a finished
/// blob is linked at before their view links it.
#[derive(Default)]
struct Downloading {
    blobs: HashSet<String>,
    entries: HashSet<Key>,
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
        let snapshot = super::pending::snapshot(store.paths(), &pending);
        for file in pending.files {
            let entry = snapshot.as_ref().map(|snapshot| snapshot.join(&file.path));
            claimed.entries.extend(entry.as_deref().and_then(snapshot::key));
            claimed.blobs.insert(file.etag);
        }
    }
    claimed
}

/// Removes the files a downloading `reference` recorded, final or partial,
/// except those a sibling variant links or another downloading one
/// records, and blobs another snapshot's link names; the bytes freed.
/// `blobs` is what the removal read of the repository's snapshot links,
/// and forgets what goes.
pub(in crate::catalog::download) fn remove_pending(
    store: &Store,
    reference: &HubReference,
    blobs: &mut Blobs,
) -> Result<u64> {
    let Some(pending) = super::pending::recorded(store.paths(), reference) else {
        return Ok(0);
    };
    let kept = kept(&siblings(store, reference)?);
    let root = store
        .paths()
        .hub_cache_dir
        .join(format!("models--{}", crate::config::model_key(reference.repository())?));
    let snapshot = root.join("snapshots").join(&pending.commit);
    let downloading = downloading(store, reference).blobs;
    let mut freed = 0;
    for ((blob, partial), file) in pending.blobs(store.paths())?.into_iter().zip(&pending.files) {
        let entry = snapshot.join(&file.path);
        let key = snapshot::key(&entry);
        // Claimed by another download; a sibling's hard link or link, or
        // its entry, which a sibling links.
        if downloading.contains(&file.etag)
            || identity(&blob).is_some_and(|blob| kept.blobs.contains(&blob))
            || blob.canonicalize().is_ok_and(|blob| kept.files.contains(&blob))
            || key.as_ref().is_some_and(|key| kept.entries.contains(key))
        {
            continue;
        }
        let named = match (root.canonicalize(), blob.canonicalize()) {
            (Ok(repository), Ok(blob)) => blobs.named(&repository, &blob, key.as_ref())?,
            _ => false,
        };
        // The snapshot's entry of a finished blob too; what is gone, done.
        // A marker kept until all of it goes, a failure is retried whole.
        let mut freed_by = |removed: std::io::Result<u64>| match removed {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            removed => removed.map(|bytes| freed += bytes),
        };
        freed_by(blobs.remove(&partial))?;
        if !named {
            freed_by(blobs.remove(&blob))?;
        }
        freed_by(blobs.remove_entry(&entry, key.as_ref()))?;
    }
    Ok(freed)
}

/// Removes the view of the variant `reference` saved under its own key.
pub(in crate::catalog::download) fn remove_files(
    store: &Store,
    reference: &HubReference,
    blobs: &mut Blobs,
) -> Result<u64> {
    let view = store.paths().gguf_variants_dir.join(reference.key());
    if !view.is_dir() {
        return Ok(0);
    }
    remove_view(store, reference, &view, blobs)
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
/// a downloading one records; the bytes freed. `blobs` is what the removal
/// read of the repository's, kept across its views and their shards.
pub(in crate::catalog::download) fn remove_view(
    store: &Store,
    reference: &HubReference,
    view: &Path,
    blobs: &mut Blobs,
) -> Result<u64> {
    let kept = kept(&siblings(store, reference)?);
    let downloading = downloading(store, reference);
    // A snapshot of another commit links the same blob at another path.
    let needed = |blob: &Blob| {
        let etag = blob.path.file_name().and_then(|etag| etag.to_str());
        etag.is_some_and(|etag| downloading.blobs.contains(etag))
            || kept.files.contains(&blob.path)
            || blob.identity.is_some_and(|blob| kept.blobs.contains(&blob))
    };
    let mut freed = 0;
    for link in fs::read_dir(view)?.flatten() {
        // The snapshot entry the view links: its blob's hard link, or a
        // link to it. A link naming no snapshot's file leaves that file.
        let Some((key, entry)) =
            snapshot::entry(&link.path()).and_then(|entry| Some((snapshot::key(&entry)?, entry)))
        else {
            continue;
        };
        // Linked by a sibling, or claimed by a download it is to publish.
        if kept.entries.contains(&key) || downloading.entries.contains(&key) {
            continue;
        }
        let blob = match blobs.of(&entry)? {
            Some(blob)
                if !needed(&blob) && !blobs.named(&blob.repository, &blob.path, Some(&key))? =>
            {
                Some(blob)
            },
            _ => None,
        };
        // Its blob kept, the entry only names it; else its bytes go with
        // whichever of the two goes last.
        match blob {
            // A hard link's blob first: should the entry then not go, it
            // still names the blob for the removal tried again.
            Some(blob) if blob.naming == Naming::HardLink => {
                freed += blobs.remove(&blob.path)?;
                freed += blobs.remove_entry(&entry, Some(&key))?;
            },
            // A link before its blob, never left naming nothing.
            Some(blob) => {
                freed += blobs.remove_entry(&entry, Some(&key))?;
                freed += blobs.remove(&blob.path)?;
            },
            None => freed += blobs.remove_entry(&entry, Some(&key))?,
        }
    }
    let view = view.canonicalize()?;
    let import = store.gguf_import_dir(&view);
    if import.exists() {
        freed += freed::remove_tree(&import)?;
    }
    Ok(freed + freed::remove_tree(&view)?)
}
