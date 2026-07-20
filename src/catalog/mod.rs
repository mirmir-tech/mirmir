mod cache;
mod cancellation;
mod download;
mod fit;
mod hub;
mod progress;
mod queue;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

pub use cache::{CachedModel, discover_cached_models, discover_cached_models_in};
pub use download::{DownloadedModel, Removal, TransferUpdate};
pub use fit::{CatalogModel, MachineMemory};
use libmir::CancellationToken;
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
            } else if external.contains(&model.id) {
                "hf_cache"
            } else {
                "remote"
            };
        }
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
        download::send(&updates, "queued", 0, None, "waiting for download slot".to_owned()).await;
        let _permit = self.transfer_queue.acquire(cancellation).await?;
        let result = download::pull(&self.store, repo_id, revision, updates, cancellation).await;
        if matches!(result, Err(Error::Cancelled)) {
            let _ = download::discard_partial(&self.store, repo_id).await?;
        }
        result
    }

    pub async fn remove(&self, repo_id: &str) -> Result<Removal> {
        if self.is_transferring(repo_id)? {
            return Err(Error::Config(format!("model `{repo_id}` is currently downloading")));
        }
        download::remove(&self.store, repo_id).await
    }

    fn start_transfer(&self, repo_id: &str) -> Result<()> {
        let mut active = self
            .active_transfers
            .lock()
            .map_err(|_| Error::Config("model transfer registry lock is poisoned".to_owned()))?;
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
        self.active_transfers
            .lock()
            .map(|active| active.contains(repo_id))
            .map_err(|_| Error::Config("model transfer registry lock is poisoned".to_owned()))
    }
}
