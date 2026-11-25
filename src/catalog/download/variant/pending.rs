//! Variants whose download has started and not finished. Every variant of a
//! repository downloads into the repository's one Hub cache, so a marker
//! beside the variant views records which variant a partial cache belongs
//! to and which of its files: the models list shows it as that variant with
//! its own bytes, whose resume pulls that variant alone, and removing it
//! removes those files.

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    catalog::resumable::{ListedFile, Pinned},
    config::{HubReference, Paths, Store, model_key},
    error::Result,
};

const EXTENSION: &str = "pending";

/// What a marker records: the variant, the commit it downloads and its files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::catalog::download) struct Pending {
    pub reference: String,
    pub commit: String,
    pub files: Vec<PendingFile>,
}

/// One file of a downloading variant, by its blob in the repository cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::catalog::download) struct PendingFile {
    pub path: String,
    pub etag: String,
    pub size: u64,
}

impl Pending {
    /// The variant's blobs, final or `.incomplete`, under `blobs`.
    pub fn blobs(&self, paths: &Paths) -> Result<Vec<(PathBuf, PathBuf)>> {
        let root = repository_root(paths, &self.reference)?;
        Ok(self
            .files
            .iter()
            .map(|file| {
                let blob = root.join("blobs").join(&file.etag);
                let partial = root.join("blobs").join(format!("{}.incomplete", file.etag));
                (blob, partial)
            })
            .collect())
    }

    /// The bytes downloaded so far.
    fn bytes(&self, paths: &Paths) -> Result<u64> {
        Ok(self
            .blobs(paths)?
            .iter()
            .zip(&self.files)
            .map(|((blob, partial), file)| {
                fs::metadata(blob)
                    .or_else(|_| fs::metadata(partial))
                    .map_or(0, |metadata| metadata.len().min(file.size))
            })
            .sum())
    }
}

fn repository_root(paths: &Paths, reference: &str) -> Result<PathBuf> {
    let repository = reference.parse::<HubReference>()?;
    Ok(paths
        .hub_cache_dir
        .join(format!("models--{}", model_key(repository.repository())?)))
}

fn marker(paths: &Paths, reference: &HubReference) -> PathBuf {
    paths.gguf_variants_dir.join(format!("{}.{EXTENSION}", reference.key()))
}

/// Records that `reference` is downloading `files` at `pinned`'s commit.
pub(super) fn begin(
    store: &Store,
    reference: &HubReference,
    pinned: &Pinned,
    files: &[ListedFile],
) -> Result<()> {
    let pending = Pending {
        reference: reference.to_string(),
        commit: pinned.commit.clone(),
        files: files
            .iter()
            .map(|file| PendingFile {
                path: file.path.clone(),
                etag: file.etag.clone(),
                size: file.size,
            })
            .collect(),
    };
    fs::create_dir_all(&store.paths().gguf_variants_dir)?;
    fs::write(marker(store.paths(), reference), serde_json::to_vec_pretty(&pending)?)?;
    Ok(())
}

/// What the marker of `reference` records; `None` without a marker, or for
/// one an earlier version wrote, which named only the variant.
pub(in crate::catalog::download) fn recorded(
    paths: &Paths,
    reference: &HubReference,
) -> Option<Pending> {
    serde_json::from_slice(&fs::read(marker(paths, reference)).ok()?).ok()
}

/// Forgets `reference`'s download: it finished, or it was removed.
pub(in crate::catalog::download) fn end(store: &Store, reference: &HubReference) -> Result<()> {
    match fs::remove_file(marker(store.paths(), reference)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// The variants whose download has not finished.
#[must_use]
pub fn pending_variants(paths: &Paths) -> Vec<HubReference> {
    let Ok(entries) = fs::read_dir(&paths.gguf_variants_dir) else {
        return Vec::new();
    };
    let mut pending = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == EXTENSION))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| {
            serde_json::from_str::<Pending>(&text)
                .map_or_else(|_| text.trim().to_owned(), |pending| pending.reference)
                .parse::<HubReference>()
                .ok()
        })
        .filter(|reference| reference.variant().is_some())
        .collect::<Vec<_>>();
    pending.sort_by_key(ToString::to_string);
    pending
}

/// The bytes `reference` has downloaded, by its own files; `None` for a
/// marker that does not list them.
#[must_use]
pub fn pending_bytes(paths: &Paths, reference: &HubReference) -> Option<u64> {
    recorded(paths, reference)?.bytes(paths).ok()
}
