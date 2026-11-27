//! Pulling and removing Hub models, one transfer per model at a time.

mod registry;
mod removal;

use libmir::{BackendTarget, CancellationToken};
pub(super) use registry::Transfers;
pub use registry::{Activity, TransferConflict};
#[cfg(test)]
use removal::PreparedRemoval;

use super::{
    Catalog, DownloadedModel, MachineMemory, ModelVariants, TransferPhase, TransferUpdate,
    discover_cached_models_in, download, fit,
    hub::{GgufRepository, RepoFile},
    resumable::{self, ListedFile},
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
        // The configurations are read once for every variant listed.
        let downloaded = self.store.downloaded_models()?;
        for variant in &mut variants {
            variant.downloaded = downloaded.contains(&reference.with_variant(&variant.name)?);
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
        let _transfer = self.reserve(repo_id, Activity::Pull)?;
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
        // Its repository as another model of it was saved: a second spelling
        // would be a second cache and configuration of one Hub repository.
        let mut reference = download::spelled(&self.store, &repo_id.parse::<HubReference>()?)?;
        let repo_id = &reference.to_string();
        // Installed while this pull waited for its slot, by another process
        // or a pull that held it.
        if self.store.hub_model_downloaded(repo_id)? {
            return Err(Error::Config(format!("model `{repo_id}` is already downloaded")));
        }
        // One commit for the choice, the header checks and every file.
        let revision =
            resumable::pin(&self.store, reference.repository(), revision.unwrap_or("main")).await?;
        let revision = &revision;
        download::send(
            &updates,
            TransferPhase::Resolving,
            0,
            None,
            format!("resolving {reference}@{} ({})", revision.requested, revision.commit),
        )
        .await;
        // One listing for the choice, the header check and the transfer.
        let files =
            resumable::listing(&self.store, reference.repository(), &revision.commit).await?;
        // The variant chosen below, or the label the one asked for has,
        // transfers under its own id too, so its row is neither removed nor
        // pulled again meanwhile.
        #[expect(clippy::collection_is_never_read, reason = "guards held until the pull ends")]
        let mut labels = Vec::new();
        // A GGUF repository pulled without a variant takes the catalog's
        // default one.
        if reference.variant().is_none()
            && let Some(chosen) = download::default_variant(&reference, &files, memory)?
        {
            if self.store.hub_model_downloaded(&chosen.to_string())? {
                return Err(Error::Config(format!("model `{chosen}` is already downloaded")));
            }
            labels.push(self.reserve(&chosen.to_string(), Activity::Pull)?);
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
                .any(|model| model.repo_id.eq_ignore_ascii_case(repo_id))
        {
            return Err(Error::Config(format!("model `{repo_id}` is already downloaded")));
        }
        if reference.variant().is_some() {
            if let Some((canonical, transfer)) = self.reserve_label(&reference, &files)? {
                labels.push(transfer);
                reference = canonical;
            }
            download::send(
                &updates,
                TransferPhase::Checking,
                0,
                None,
                "validating the remote GGUF header".to_owned(),
            )
            .await;
            let pinned = (revision, files.as_slice());
            download::check_variant(&self.store, &self.client, &reference, pinned).await?;
            let chosen = reference.to_string();
            let listed = (revision, files);
            return download::pull(&self.store, &chosen, listed, updates, cancellation).await;
        }
        download::send(
            &updates,
            TransferPhase::Checking,
            0,
            None,
            "validating remote SafeTensors headers".to_owned(),
        )
        .await;
        self.check_safetensors(repo_id, revision).await?;
        download::pull(&self.store, repo_id, (revision, files), updates, cancellation).await
    }

    /// Reserves the variant `reference` names under its label in the pinned
    /// `files` too, when it was asked by another spelling or its file stem:
    /// it downloads, before any marker or byte is written, and is removed
    /// under that label, which a removal must then find taken.
    fn reserve_label(
        &self,
        reference: &HubReference,
        files: &[ListedFile],
    ) -> Result<Option<(HubReference, registry::Transfer<'_>)>> {
        let canonical = download::canonical_variant(reference, files)?;
        if canonical.is_same_model(reference) {
            return Ok(None);
        }
        if self.store.hub_model_downloaded(&canonical.to_string())? {
            return Err(Error::Config(format!("model `{canonical}` is already downloaded")));
        }
        let transfer = self.reserve(&canonical.to_string(), Activity::Pull)?;
        Ok(Some((canonical, transfer)))
    }

    /// Admits a `SafeTensors` repository from its remote headers before it
    /// downloads, logging what they tell.
    async fn check_safetensors(&self, repo_id: &str, revision: &resumable::Pinned) -> Result<()> {
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
        Ok(())
    }
}

#[cfg(test)]
mod tests;
