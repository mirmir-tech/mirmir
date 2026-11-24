//! Pulling and removing Hub models, one transfer per model at a time.

use libmir::{BackendTarget, CancellationToken};

use super::{
    Catalog, DownloadedModel, Removal, TransferPhase, TransferUpdate, discover_cached_models_in,
    download,
};
use crate::{
    config::HubReference,
    error::{Error, Result},
};

impl Catalog {
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
        let reference = repo_id.parse::<HubReference>()?;
        if reference.variant().is_some() {
            download::send(
                &updates,
                TransferPhase::Checking,
                0,
                None,
                "validating the remote GGUF header".to_owned(),
            )
            .await;
            download::check_variant(&self.store, &self.client, &reference, revision).await?;
            return download::pull(&self.store, repo_id, revision, updates, cancellation).await;
        }
        download::send(
            &updates,
            TransferPhase::Checking,
            0,
            None,
            "validating remote SafeTensors headers".to_owned(),
        )
        .await;
        download::refuse_gguf_repository(&self.store, &reference, revision).await?;
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
