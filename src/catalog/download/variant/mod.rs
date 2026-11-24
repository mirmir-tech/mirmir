//! One GGUF variant of a Hub repository: downloaded alone with the vision
//! tower, seen through a directory linking its snapshot files, and removed
//! without the files another downloaded variant still uses.

use std::{
    fs,
    path::{Path, PathBuf},
};

use libmir::CancellationToken;
pub(super) use remove::{remove_files, siblings};
use tokio::sync::mpsc;

mod remove;
#[cfg(test)]
mod tests;

use super::{DownloadedModel, TransferPhase, TransferUpdate, directory_size, send};
use crate::{
    catalog::{
        hub::{GgufRepository, GgufVariant, RepoFile},
        preflight,
        resumable::{self, ListedFile},
    },
    config::{HubReference, Store},
    error::{Error, Result},
};

/// A listing's GGUF files grouped into variants.
pub(super) fn repository(files: &[ListedFile]) -> GgufRepository {
    let files = files
        .iter()
        .map(|file| RepoFile { path: file.path.clone(), size: file.size })
        .collect::<Vec<_>>();
    GgufRepository::from_files(&files)
}

/// Whether a listing is GGUF variants only, with no `SafeTensors` weights
/// to download as one model.
pub(super) fn gguf_only(files: &[ListedFile], repository: &GgufRepository) -> bool {
    !repository.variants.is_empty()
        && !files
            .iter()
            .any(|file| file.path.to_ascii_lowercase().ends_with(".safetensors"))
}

/// Refuses a repository of GGUF variants pulled without one, naming them.
pub(in crate::catalog) async fn refuse_gguf_repository(
    store: &Store,
    reference: &HubReference,
    revision: Option<&str>,
) -> Result<()> {
    let files =
        resumable::listing(store, reference.repository(), revision.unwrap_or("main")).await?;
    let gguf = repository(&files);
    if gguf_only(&files, &gguf) {
        return Err(Error::Config(format!(
            "`{reference}` holds one GGUF file per quantization; pull one as \
             `{reference}:VARIANT`, from {}",
            gguf.names()
        )));
    }
    Ok(())
}

/// Admits the variant from its remote header before it downloads.
pub(in crate::catalog) async fn check_variant(
    store: &Store,
    client: &reqwest::Client,
    reference: &HubReference,
    revision: Option<&str>,
) -> Result<()> {
    let revision = revision.unwrap_or("main");
    let gguf = repository(&resumable::listing(store, reference.repository(), revision).await?);
    let variant = named(&gguf, reference)?;
    let token = store.hf_token()?;
    let (contract, fetched) = preflight::inspect_gguf(
        client,
        token.as_deref(),
        reference.repository(),
        revision,
        variant,
    )
    .await?;
    // A header libmir refuses would not import after the download either.
    let contract = contract?;
    tracing::info!(
        model = %reference,
        revision,
        shards = variant.shards.len(),
        bytes = variant.bytes(),
        vision_tower = gguf.vision.is_some(),
        metal_compatibility = contract.admission(libmir::BackendTarget::Metal).status.as_str(),
        cuda_compatibility = contract.admission(libmir::BackendTarget::Cuda).status.as_str(),
        fetched_bytes = fetched,
        "remote GGUF header validated"
    );
    Ok(())
}

/// The variant `reference` names, or an error naming the ones offered.
fn named<'a>(gguf: &'a GgufRepository, reference: &HubReference) -> Result<&'a GgufVariant> {
    let name = reference.variant().unwrap_or_default();
    gguf.variant(name).ok_or_else(|| {
        Error::Config(format!(
            "`{}` has no GGUF variant `{name}`; it offers {}",
            reference.repository(),
            gguf.names()
        ))
    })
}

pub(super) async fn pull(
    store: &Store,
    reference: &HubReference,
    (revision, files): (&str, Vec<ListedFile>),
    updates: &mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<DownloadedModel> {
    let gguf = repository(&files);
    let variant = named(&gguf, reference)?;
    // The variant under its label, whatever case it was asked in.
    let reference = reference.with_variant(&variant.name)?;
    let wanted = variant
        .shards
        .iter()
        .chain(gguf.vision.as_ref())
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    let commit = resumable::commit(store, reference.repository(), revision, &files).await?;
    let selected = files.into_iter().filter(|file| wanted.contains(&file.path)).collect();
    let snapshot = resumable::snapshot(
        store,
        (reference.repository(), revision, commit),
        selected,
        updates,
        cancellation,
    )
    .await?;
    let view = link(store, &reference, &snapshot, &wanted)?;
    send(
        updates,
        TransferPhase::Validating,
        directory_size(&view),
        None,
        format!("importing {reference}"),
    )
    .await;
    let load_unavailable_reason = match store.gguf_import(&view) {
        Ok(Some(import)) => {
            libmir::ModelDescriptor::inspect(&import, libmir::GenerationOverrides::default())
                .err()
                .map(|error| error.to_string())
        },
        Ok(None) => Some("the variant holds no GGUF text model".to_owned()),
        Err(error) => Some(error.to_string()),
    };
    let commit = snapshot
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config("Hub snapshot has no commit directory".to_owned()))?;
    let config = store.save_hub_model(&reference.to_string(), revision, commit, view)?;
    let message = load_unavailable_reason.as_ref().map_or_else(
        || format!("saved model configuration for {reference}"),
        |reason| format!("downloaded {reference}; loading is unavailable: {reason}"),
    );
    send(updates, TransferPhase::Available, gguf_bytes(&wanted, &gguf), None, message).await;
    Ok(DownloadedModel { config, load_unavailable_reason })
}

fn gguf_bytes(wanted: &[String], gguf: &GgufRepository) -> u64 {
    let vision = gguf.vision.iter();
    let files = gguf.variants.iter().flat_map(|variant| variant.shards.iter()).chain(vision);
    files.filter(|file| wanted.contains(&file.path)).map(|file| file.size).sum()
}

/// The variant's directory: a link per file into `snapshot`, recreated.
fn link(
    store: &Store,
    reference: &HubReference,
    snapshot: &Path,
    files: &[String],
) -> Result<PathBuf> {
    let view = store.paths().gguf_variants_dir.join(reference.key());
    if view.exists() {
        fs::remove_dir_all(&view)?;
    }
    fs::create_dir_all(&view)?;
    for file in files {
        let name = file.rsplit('/').next().unwrap_or(file);
        symlink(&snapshot.join(file), &view.join(name))?;
    }
    Ok(view.canonicalize()?)
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
