mod cache;
mod cancellation;
mod download;
mod fit;
mod hub;
mod preflight;
mod pull;
mod queue;
mod resumable;
mod search_preflight;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

pub use cache::{
    CachedModel, PartialDownload, cached_repository_bytes, discover_cached_models_in,
    discover_external_models, discover_partial_downloads_in,
};
pub use download::{
    DownloadedModel, Removal, TransferPhase, TransferUpdate, pending_bytes, pending_variants,
};
pub use fit::{
    CatalogModel, CatalogVariant, Confidence, MachineMemory, MemoryFit, VariantSize,
    admission_status, rank,
};
pub use hub::{HubFacts, encoding_named};
pub use preflight::RemoteHeaderPreflight;
use queue::TransferQueue;

use crate::{
    config::{HubReference, Store},
    error::{Error, Result},
};

#[derive(Clone)]
pub struct Catalog {
    client: reqwest::Client,
    store: Store,
    active_transfers: Arc<Mutex<pull::Transfers>>,
    transfer_queue: TransferQueue,
    preflight_cache: preflight::Cache,
    /// Removals run one at a time: whether a removal leaves its repository
    /// with nothing is decided by the others having finished.
    removals: Arc<tokio::sync::Mutex<()>>,
}

/// A GGUF repository's variants with their listed sizes.
pub struct ModelVariants {
    pub variants: Vec<CatalogVariant>,
    pub default_variant: Option<String>,
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
            active_transfers: Arc::new(Mutex::new(pull::Transfers::new())),
            transfer_queue: TransferQueue::serial(),
            preflight_cache: preflight::Cache::default(),
            removals: Arc::default(),
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
        let external = discover_external_models(self.store.paths())
            .into_iter()
            .map(|model| model.repo_id.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let managed = discover_cached_models_in(&self.store.paths().hub_cache_dir)
            .into_iter()
            .map(|model| model.repo_id.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let partial = discover_partial_downloads_in(&self.store.paths().hub_cache_dir)
            .into_iter()
            .map(|model| model.repo_id.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let mut models = page
            .models
            .into_iter()
            .map(|candidate| fit::evaluate(candidate, memory))
            .collect::<Vec<_>>();
        // The configurations are read once for every model and variant.
        let downloaded = self.store.downloaded_models()?;
        // Caches by repository in any spelling, as the Hub resolves it.
        for model in &mut models {
            let reference = model.id.parse::<HubReference>()?;
            let id = model.id.to_ascii_lowercase();
            for variant in &mut model.variants {
                variant.downloaded = downloaded.contains(&reference.with_variant(&variant.name)?);
            }
            model.downloaded = model.target_downloaded(|| {
                Ok(downloaded.contains(&reference) || managed.contains(&id))
            })?;
            model.local_source = if model.downloaded {
                "mirmir"
            } else if partial.contains(&id) {
                "partial"
            } else if external.contains(&id) {
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
}
