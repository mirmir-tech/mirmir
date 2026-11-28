//! Removing a GGUF variant: its recorded snapshot files and the blobs no other
//! downloaded variant of the repository still uses.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use super::{
    blobs::{Blob, Blobs, Identity, Naming, identity},
    snapshot::{self, Key},
};
use crate::{
    config::{HubReference, ModelConfig, Store, model_key},
    error::Result,
};

/// Saved models from `reference`'s repository other than itself, by the
/// references their configurations record: other variants and the repository's
/// own snapshot, whose files a variant may share.
pub(in crate::catalog::download) fn siblings(
    store: &Store,
    reference: &HubReference,
) -> Result<Vec<ModelConfig>> {
    Ok(store
        .list_model_configs()?
        .into_iter()
        .filter(|model| {
            model
                .hub
                .as_ref()
                .and_then(|hub| hub.repo_id.parse::<HubReference>().ok())
                .is_some_and(|other| {
                    other.same_repository(reference) && !other.is_same_model(reference)
                })
        })
        .collect())
}

/// Snapshot entries owned by a saved model configuration: for a GGUF variant
/// with recorded `files`, those paths in its snapshot; for a standalone `.gguf`
/// file, that file (plus optional `vision`); for a directory snapshot, every
/// file beneath it.
fn owned_entries(model: &ModelConfig) -> Vec<PathBuf> {
    if let Some(hub) = &model.hub
        && !hub.files.is_empty()
        && let Some(snapshot) = snapshot_root(&model.path)
    {
        return hub.files.iter().map(|file| snapshot.join(file)).collect();
    }
    if model.path.is_dir() {
        let mut files = Vec::new();
        let mut dirs = vec![model.path.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    dirs.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        return files;
    }
    let mut files = vec![model.path.clone()];
    if let Some(vision) = model.hub.as_ref().and_then(|hub| hub.vision.as_ref()) {
        files.push(vision.clone());
    }
    files
}

fn snapshot_root(model_path: &Path) -> Option<PathBuf> {
    for ancestor in model_path.ancestors() {
        if ancestor.parent().and_then(Path::file_name) == Some("snapshots".as_ref()) {
            return Some(ancestor.to_path_buf());
        }
    }
    model_path.parent().map(Path::to_path_buf)
}

/// What the `siblings` hold: their files canonical, the snapshot entries they
/// own, and their files' identities, which a blob hard-linked or linked at
/// another commit shares.
#[derive(Default)]
struct Kept {
    files: HashSet<PathBuf>,
    entries: HashSet<Key>,
    blobs: HashSet<Identity>,
}

fn kept(siblings: &[ModelConfig]) -> Kept {
    let mut kept = Kept::default();
    for sibling in siblings {
        for path in owned_entries(sibling) {
            kept.files.extend(path.canonicalize().ok());
            kept.entries.extend(snapshot::entry(&path).as_deref().and_then(snapshot::key));
            kept.blobs.extend(identity(&path));
        }
    }
    kept
}

/// What the other variants of a repository still downloading claim: their
/// blobs, by `ETag`, and their snapshot entries.
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
/// except those a sibling variant owns or another downloading one records, and
/// blobs another snapshot's link names; returns the bytes freed.
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
        .join(format!("models--{}", model_key(reference.repository())?));
    let snapshot = root.join("snapshots").join(&pending.commit);
    let downloading = downloading(store, reference).blobs;
    let mut freed = 0;
    for ((blob, partial), file) in pending.blobs(store.paths())?.into_iter().zip(&pending.files) {
        let entry = snapshot.join(&file.path);
        let key = snapshot::key(&entry);
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

/// Removes the snapshot files and unshared blobs owned by a saved variant
/// `model`; returns the bytes freed.
pub(in crate::catalog::download) fn remove_variant(
    store: &Store,
    reference: &HubReference,
    model: &ModelConfig,
    blobs: &mut Blobs,
) -> Result<u64> {
    let kept = kept(&siblings(store, reference)?);
    let downloading = downloading(store, reference);
    let needed = |blob: &Blob| {
        let etag = blob.path.file_name().and_then(|etag| etag.to_str());
        etag.is_some_and(|etag| downloading.blobs.contains(etag))
            || kept.files.contains(&blob.path)
            || blob.identity.is_some_and(|blob| kept.blobs.contains(&blob))
    };
    let mut freed = 0;
    for path in owned_entries(model) {
        let Some((key, entry)) =
            snapshot::entry(&path).and_then(|entry| Some((snapshot::key(&entry)?, entry)))
        else {
            continue;
        };
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
        match blob {
            Some(blob) if blob.naming == Naming::HardLink => {
                freed += blobs.remove(&blob.path)?;
                freed += blobs.remove_entry(&entry, Some(&key))?;
            },
            Some(blob) => {
                freed += blobs.remove_entry(&entry, Some(&key))?;
                freed += blobs.remove(&blob.path)?;
            },
            None => freed += blobs.remove_entry(&entry, Some(&key))?,
        }
    }
    Ok(freed)
}

#[cfg(test)]
pub(super) fn remove_files(
    store: &Store,
    reference: &HubReference,
    blobs: &mut Blobs,
) -> Result<u64> {
    let mut freed = 0;
    for model in store.list_model_configs()? {
        if model
            .hub
            .as_ref()
            .and_then(|hub| hub.repo_id.parse::<HubReference>().ok())
            .is_some_and(|saved| saved.is_same_model(reference))
        {
            freed += remove_variant(store, reference, &model, blobs)?;
        }
    }
    Ok(freed)
}
