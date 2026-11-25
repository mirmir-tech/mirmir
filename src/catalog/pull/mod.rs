//! Pulling and removing Hub models, one transfer per model at a time.

use libmir::{BackendTarget, CancellationToken};

use super::{
    Catalog, DownloadedModel, MachineMemory, ModelVariants, Removal, TransferPhase, TransferUpdate,
    discover_cached_models_in, download, fit,
    hub::{GgufRepository, RepoFile},
    resumable,
};
use crate::{
    config::HubReference,
    error::{Error, Result},
};

impl Catalog {
    /// `repo_id`'s GGUF variants with the sizes of their listed files.
    pub async fn variants(&self, repo_id: &str, memory: MachineMemory) -> Result<ModelVariants> {
        let reference = repo_id.parse::<HubReference>()?;
        let files = resumable::listing(&self.store, reference.repository(), "main").await?;
        let files = files
            .into_iter()
            .map(|file| RepoFile { path: file.path, size: Some(file.size) })
            .collect::<Vec<_>>();
        let gguf = GgufRepository::from_files(&files);
        // Variants only for a repository a pull treats as GGUF-only, as the
        // search shows it.
        if !gguf.gguf_only() {
            return Ok(ModelVariants {
                variants: Vec::new(),
                default_variant: None,
            });
        }
        let (mut variants, default_variant) = fit::variants(&gguf, None, memory);
        for variant in &mut variants {
            let id = reference.with_variant(&variant.name)?;
            variant.downloaded = self.store.hub_model_downloaded(&id.to_string())?;
        }
        Ok(ModelVariants { variants, default_variant })
    }

    /// Pulls `repo_id`; a GGUF repository without a variant takes the
    /// default the search shows for `memory`.
    pub async fn pull(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        memory: MachineMemory,
        updates: tokio::sync::mpsc::Sender<TransferUpdate>,
        cancellation: &CancellationToken,
    ) -> Result<DownloadedModel> {
        if self.store.hub_model_downloaded(repo_id)? {
            return Err(Error::Config(format!("model `{repo_id}` is already downloaded")));
        }
        let _transfer = self.reserve(repo_id)?;
        self.pull_reserved(repo_id, (revision, memory), updates, cancellation).await
    }

    async fn pull_reserved(
        &self,
        repo_id: &str,
        (revision, memory): (Option<&str>, MachineMemory),
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
        let mut reference = repo_id.parse::<HubReference>()?;
        // One commit for the choice, the header checks and every file.
        let revision =
            resumable::pin(&self.store, reference.repository(), revision.unwrap_or("main")).await?;
        let revision = &revision;
        // The variant chosen below transfers under its own id too, so its
        // row is neither removed nor pulled again meanwhile.
        #[expect(clippy::collection_is_never_read, reason = "a guard held until the pull ends")]
        let mut _chosen = None;
        // A GGUF repository pulled without a variant takes the catalog's
        // default one.
        if reference.variant().is_none()
            && let Some(chosen) =
                download::default_variant(&self.store, &reference, revision, memory).await?
        {
            if self.store.hub_model_downloaded(&chosen.to_string())? {
                return Err(Error::Config(format!("model `{chosen}` is already downloaded")));
            }
            _chosen = Some(self.reserve(&chosen.to_string())?);
            download::send(
                &updates,
                TransferPhase::Checking,
                0,
                None,
                format!("choosing {chosen}, the default of {reference}"),
            )
            .await;
            reference = chosen;
        }
        // A whole-repository snapshot already in the cache is that model;
        // a variant's snapshot of a GGUF-only repository is not.
        if reference.variant().is_none()
            && discover_cached_models_in(&self.store.paths().hub_cache_dir)
                .into_iter()
                .any(|model| model.repo_id == repo_id)
        {
            return Err(Error::Config(format!("model `{repo_id}` is already downloaded")));
        }
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
            let chosen = reference.to_string();
            return download::pull(&self.store, &chosen, revision, updates, cancellation).await;
        }
        download::send(
            &updates,
            TransferPhase::Checking,
            0,
            None,
            "validating remote SafeTensors headers".to_owned(),
        )
        .await;
        let preflight = self.preflight_headers(repo_id, Some(&revision.commit)).await?;
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

    /// Registers `repo_id`'s transfer until the returned guard drops.
    fn reserve(&self, repo_id: &str) -> Result<Transfer<'_>> {
        let Ok(mut active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        if !active.insert(repo_id.to_owned()) {
            return Err(Error::Config(format!("model `{repo_id}` is already downloading")));
        }
        drop(active);
        Ok(Transfer {
            catalog: self,
            repo_id: repo_id.to_owned(),
        })
    }

    fn is_transferring(&self, repo_id: &str) -> Result<bool> {
        let Ok(active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        Ok(active.contains(repo_id))
    }
}

/// A registered transfer, forgotten when dropped, however the pull ends.
struct Transfer<'a> {
    catalog: &'a Catalog,
    repo_id: String,
}

impl Drop for Transfer<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.catalog.active_transfers.lock() {
            active.remove(&self.repo_id);
        }
    }
}

#[cfg(test)]
mod tests;
