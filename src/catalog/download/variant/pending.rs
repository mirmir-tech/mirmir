//! Variants whose download has started and not finished. Every variant of a
//! repository downloads into the repository's one Hub cache, so a marker
//! beside the variant views records which variant a partial cache belongs
//! to: the models list shows it as that variant, whose resume pulls that
//! variant alone, instead of as the whole repository.

use std::{fs, path::PathBuf};

use crate::{
    config::{HubReference, Paths, Store},
    error::Result,
};

const EXTENSION: &str = "pending";

fn marker(paths: &Paths, reference: &HubReference) -> PathBuf {
    paths.gguf_variants_dir.join(format!("{}.{EXTENSION}", reference.key()))
}

/// Records that `reference` is downloading.
pub(super) fn begin(store: &Store, reference: &HubReference) -> Result<()> {
    fs::create_dir_all(&store.paths().gguf_variants_dir)?;
    fs::write(marker(store.paths(), reference), reference.to_string())?;
    Ok(())
}

/// Forgets `reference`'s download: it finished, or it was removed.
pub(in crate::catalog::download) fn end(store: &Store, reference: &HubReference) -> Result<()> {
    match fs::remove_file(marker(store.paths(), reference)) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// The variants of `repository` whose download has not finished.
#[must_use]
pub fn pending_variants(paths: &Paths, repository: &str) -> Vec<HubReference> {
    let Ok(entries) = fs::read_dir(&paths.gguf_variants_dir) else {
        return Vec::new();
    };
    let mut pending = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == EXTENSION))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| text.trim().parse::<HubReference>().ok())
        .filter(|reference| reference.repository() == repository && reference.variant().is_some())
        .collect::<Vec<_>>();
    pending.sort_by_key(ToString::to_string);
    pending
}
