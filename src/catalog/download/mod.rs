mod freed;
mod remove;
mod spelling;
mod variant;

use hf_hub::HFClient;
use libmir::CancellationToken;
pub use remove::{remove, remove_emptied};
pub(super) use spelling::spelled;
use tokio::sync::mpsc;
pub(super) use variant::{canonical_variant, check_variant, default_variant};
pub use variant::{pending_bytes, pending_variants};

use super::resumable::{self, ListedFile, Pinned};
use crate::{
    config::{HubReference, ModelConfig, Store},
    error::{Error, Result},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPhase {
    Queued,
    Checking,
    Resolving,
    Downloading,
    Validating,
    Available,
}

#[derive(Debug, Clone)]
pub struct TransferUpdate {
    pub phase: TransferPhase,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DownloadedModel {
    pub config: ModelConfig,
    pub load_unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Removal {
    pub removed: bool,
    pub freed_bytes: u64,
}

/// Downloads `repo_id` at `pinned`'s commit, the one every request uses,
/// from the repository's `files` listed at that commit.
pub async fn pull(
    store: &Store,
    repo_id: &str,
    (pinned, files): (&Pinned, Vec<ListedFile>),
    updates: mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<DownloadedModel> {
    let reference = repo_id.parse::<HubReference>()?;
    if reference.variant().is_some() {
        return variant::pull(store, &reference, (pinned, files), &updates, cancellation).await;
    }
    let snapshot =
        resumable::snapshot(store, (repo_id, pinned), files, &updates, cancellation).await?;
    send(
        &updates,
        TransferPhase::Validating,
        directory_size(&snapshot),
        None,
        format!("validating snapshot for {repo_id}"),
    )
    .await;
    let inspection =
        libmir::ModelDescriptor::inspect(&snapshot, libmir::GenerationOverrides::default());
    let load_unavailable_reason = inspection.err().map(|error| error.to_string());
    let commit = snapshot
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::Config("Hub snapshot has no commit directory".to_owned()))?;
    let model = store.save_hub_model(repo_id, &pinned.requested, commit, snapshot.clone())?;
    let message = load_unavailable_reason.as_ref().map_or_else(
        || format!("saved model configuration for {repo_id}"),
        |reason| format!("downloaded {repo_id}; loading is unavailable: {reason}"),
    );
    send(&updates, TransferPhase::Available, directory_size(&snapshot), None, message).await;
    Ok(DownloadedModel { config: model, load_unavailable_reason })
}

pub(super) fn client(store: &Store) -> Result<HFClient> {
    let mut builder = HFClient::builder()
        .cache_dir(&store.paths().hub_cache_dir)
        .user_agent(concat!("mirmir/", env!("CARGO_PKG_VERSION")));
    if let Some(token) = store.hf_token()? {
        builder = builder.token(token);
    }
    Ok(builder.build()?)
}

pub(super) async fn send(
    sender: &mpsc::Sender<TransferUpdate>,
    phase: TransferPhase,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    message: String,
) {
    drop(
        sender
            .send(TransferUpdate {
                phase,
                downloaded_bytes,
                total_bytes,
                message,
            })
            .await,
    );
}

fn directory_size(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path).map_or(0, |entries| {
        entries.flatten().fold(0, |total, entry| {
            let path = entry.path();
            total.saturating_add(if path.is_dir() {
                directory_size(&path)
            } else {
                entry.metadata().map_or(0, |metadata| metadata.len())
            })
        })
    })
}

#[cfg(test)]
mod tests;
