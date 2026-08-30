mod cache;
mod cancellation;
mod download;
mod fit;
mod hub;
mod preflight;
mod queue;
mod resumable;
mod search_preflight;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

pub use cache::{
    CachedModel, PartialDownload, discover_cached_models, discover_cached_models_in,
    discover_partial_downloads_in,
};
pub use download::{DownloadedModel, Removal, TransferPhase, TransferUpdate};
pub use fit::{CatalogModel, MachineMemory};
use libmir::{BackendTarget, CancellationToken};
pub use preflight::RemoteHeaderPreflight;
use queue::TransferQueue;

use crate::{
    config::Store,
    error::{Error, Result},
};

#[derive(Clone)]
pub struct Catalog {
    client: reqwest::Client,
    store: Store,
    active_transfers: Arc<Mutex<HashSet<String>>>,
    transfer_queue: TransferQueue,
    preflight_cache: preflight::Cache,
}

pub struct SearchResults {
    pub models: Vec<CatalogModel>,
    pub memory: MachineMemory,
    pub next_cursor: Option<String>,
}

impl Catalog {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self {
            client: reqwest::Client::new(),
            store,
            active_transfers: Arc::new(Mutex::new(HashSet::new())),
            transfer_queue: TransferQueue::serial(),
            preflight_cache: preflight::Cache::default(),
        }
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        memory: MachineMemory,
        cursor: Option<&str>,
    ) -> Result<SearchResults> {
        let token = self.store.hf_token()?;
        let page = hub::search(&self.client, token.as_deref(), query, limit, cursor).await?;
        let external = discover_cached_models()
            .into_iter()
            .map(|model| model.repo_id)
            .collect::<HashSet<_>>();
        let managed = discover_cached_models_in(&self.store.paths().hub_cache_dir)
            .into_iter()
            .map(|model| model.repo_id)
            .collect::<HashSet<_>>();
        let partial = discover_partial_downloads_in(&self.store.paths().hub_cache_dir)
            .into_iter()
            .map(|model| model.repo_id)
            .collect::<HashSet<_>>();
        let mut models = page
            .models
            .into_iter()
            .map(|candidate| fit::evaluate(candidate, memory))
            .collect::<Vec<_>>();
        for model in &mut models {
            model.downloaded =
                self.store.hub_model_downloaded(&model.id)? || managed.contains(&model.id);
            model.local_source = if model.downloaded {
                "mirmir"
            } else if partial.contains(&model.id) {
                "partial"
            } else if external.contains(&model.id) {
                "hf_cache"
            } else {
                "remote"
            };
        }
        search_preflight::enrich(self, &mut models).await;
        models.sort_by(fit::compare);
        Ok(SearchResults {
            models,
            memory,
            next_cursor: page.next_cursor,
        })
    }

    pub async fn test_hf_token(&self) -> Result<String> {
        let token = self
            .store
            .hf_token()?
            .ok_or_else(|| Error::Config("Hugging Face token is not configured".to_owned()))?;
        hub::whoami(&self.client, &token).await
    }

    pub async fn preflight_headers(
        &self,
        repo_id: &str,
        revision: Option<&str>,
    ) -> Result<RemoteHeaderPreflight> {
        let token = self.store.hf_token()?;
        let revision =
            preflight::resolve_revision(&self.store, repo_id, revision.unwrap_or("main")).await?;
        if let Some(cached) = self.preflight_cache.get(repo_id, &revision)? {
            return Ok(cached);
        }
        let result = preflight::inspect_repository(
            &self.client,
            &self.store,
            token.as_deref(),
            repo_id,
            &revision,
        )
        .await?;
        self.preflight_cache.insert(repo_id, &revision, result.clone())?;
        Ok(result)
    }

    pub async fn pull(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        updates: tokio::sync::mpsc::Sender<TransferUpdate>,
        cancellation: &CancellationToken,
    ) -> Result<DownloadedModel> {
        let managed = discover_cached_models_in(&self.store.paths().hub_cache_dir)
            .into_iter()
            .any(|model| model.repo_id == repo_id);
        if self.store.hub_model_downloaded(repo_id)? || managed {
            return Err(Error::Config(format!("model `{repo_id}` is already downloaded")));
        }
        self.start_transfer(repo_id)?;
        let result = self.pull_reserved(repo_id, revision, updates, cancellation).await;
        self.finish_transfer(repo_id);
        result
    }

    async fn pull_reserved(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        updates: tokio::sync::mpsc::Sender<TransferUpdate>,
        cancellation: &CancellationToken,
    ) -> Result<DownloadedModel> {
        download::send(
            &updates,
            TransferPhase::Queued,
            0,
            None,
            "waiting for download slot".to_owned(),
        )
        .await;
        let _permit = self.transfer_queue.acquire(cancellation).await?;
        download::send(
            &updates,
            TransferPhase::Checking,
            0,
            None,
            "validating remote SafeTensors headers".to_owned(),
        )
        .await;
        let preflight = self.preflight_headers(repo_id, revision).await?;
        let model_type = preflight
            .metadata
            .config
            .get("model_type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let metal = preflight
            .contract
            .as_ref()
            .map_or("unknown", |contract| contract.admission(BackendTarget::Metal).status.as_str());
        let cuda = preflight
            .contract
            .as_ref()
            .map_or("unknown", |contract| contract.admission(BackendTarget::Cuda).status.as_str());
        tracing::info!(
            model = repo_id,
            revision = preflight.revision,
            model_type,
            tokenizer_metadata = preflight.metadata.tokenizer_config.is_some(),
            tokenizer_kind = ?preflight.metadata.tokenizer_assets.kind,
            tokenizer_bytes = preflight.metadata.tokenizer_assets.total_bytes,
            vision_pipeline = ?preflight.contract.as_ref().and_then(libmir::RemoteModelContract::vision).map(|vision| vision.config().pipeline()),
            vision_ready = preflight.contract.as_ref().and_then(libmir::RemoteModelContract::vision).is_some_and(|vision| vision.readiness().is_ready() && vision.processor().is_some()),
            metal_compatibility = metal,
            cuda_compatibility = cuda,
            contract_error = preflight.contract_error.as_deref(),
            files = preflight.files.len(),
            tensors = preflight.catalog.len(),
            fetched_bytes = preflight.fetched_bytes,
            "remote SafeTensors headers validated"
        );
        download::pull(&self.store, repo_id, revision, updates, cancellation).await
    }

    pub async fn remove(&self, repo_id: &str) -> Result<Removal> {
        if self.is_transferring(repo_id)? {
            return Err(Error::Config(format!("model `{repo_id}` is currently downloading")));
        }
        download::remove(&self.store, repo_id).await
    }

    fn start_transfer(&self, repo_id: &str) -> Result<()> {
        let Ok(mut active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        if !active.insert(repo_id.to_owned()) {
            return Err(Error::Config(format!("model `{repo_id}` is already downloading")));
        }
        drop(active);
        Ok(())
    }

    fn finish_transfer(&self, repo_id: &str) {
        if let Ok(mut active) = self.active_transfers.lock() {
            active.remove(repo_id);
        }
    }

    fn is_transferring(&self, repo_id: &str) -> Result<bool> {
        let Ok(active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        Ok(active.contains(repo_id))
    }
}
