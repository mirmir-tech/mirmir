use hf_hub::HFClient;
use libmir::CancellationToken;
use tokio::sync::mpsc;

use super::{cache::discover_cached_models, resumable};
use crate::{
    config::{ModelConfig, Store, model_key},
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

pub async fn pull(
    store: &Store,
    repo_id: &str,
    revision: Option<&str>,
    updates: mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<DownloadedModel> {
    drop(model_key(repo_id)?);
    let revision = revision.unwrap_or("main");
    send(
        &updates,
        TransferPhase::Resolving,
        0,
        None,
        format!("resolving {repo_id}@{revision}"),
    )
    .await;
    let snapshot = resumable::snapshot(store, repo_id, revision, &updates, cancellation).await?;
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
    let model = store.save_hub_model(repo_id, revision, snapshot.clone())?;
    let message = load_unavailable_reason.as_ref().map_or_else(
        || format!("saved model configuration for {repo_id}"),
        |reason| format!("downloaded {repo_id}; loading is unavailable: {reason}"),
    );
    send(&updates, TransferPhase::Available, directory_size(&snapshot), None, message).await;
    Ok(DownloadedModel { config: model, load_unavailable_reason })
}

pub async fn remove(store: &Store, repo_id: &str) -> Result<Removal> {
    let key = model_key(repo_id)?;
    let cache = client(store)?.scan_cache().send().await?;
    let managed = cache
        .repos
        .into_iter()
        .find(|repo| repo.repo_type == "model" && repo.repo_id == repo_id);
    let external = || {
        discover_cached_models()
            .into_iter()
            .find(|model| model.repo_id == repo_id)
            .map(|model| (model.repo_path, hf_hub::resolve_cache_dir(), None))
    };
    let target = managed
        .map(|repo| (repo.repo_path, store.paths().hub_cache_dir.clone(), None))
        .or_else(external)
        .or_else(|| {
            let cache = store.paths().hub_cache_dir.clone();
            let repo = cache.join(format!("models--{key}"));
            repo.exists().then_some((repo, cache, None))
        });
    let removed_cache = target.is_some();
    let freed_bytes = if let Some((path, root, known_size)) = target {
        remove_cache_repo(path, root, known_size).await?
    } else {
        0
    };
    let removed_config = store.remove_hub_model_config(repo_id)?;
    Ok(Removal {
        removed: removed_cache || removed_config,
        freed_bytes,
    })
}

async fn remove_cache_repo(
    path: std::path::PathBuf,
    cache: std::path::PathBuf,
    known_size: Option<u64>,
) -> Result<u64> {
    tokio::task::spawn_blocking(move || {
        let path = validated_cache_repo(&path, &cache)?;
        let size = known_size.unwrap_or_else(|| directory_size(&path));
        std::fs::remove_dir_all(path)?;
        Ok(size)
    })
    .await?
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

fn validated_cache_repo(
    path: &std::path::Path,
    cache: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let cache = cache.canonicalize()?;
    let path = path.canonicalize()?;
    let is_model_repo = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("models--"));
    if path.parent() == Some(cache.as_path()) && is_model_repo {
        return Ok(path);
    }
    Err(Error::Config(format!(
        "refusing to remove model cache outside {}",
        cache.display()
    )))
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
