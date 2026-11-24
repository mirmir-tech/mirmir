use libmir::{EmbeddingOutput, EmbeddingRequest, ProgressEvent, RerankOutput, RerankRequest};
use tokio::sync::mpsc;

use super::NativeRuntime;
use crate::{
    application::{
        CatalogPort, ConfigurationChange, ConfigurationOutcome, ConfigurationPort, ContextChoice,
        Error, LoadDefaults, LocalModelInfo, ModelDefaults, ModelEntry, ModelInfo, ModelInspection,
        ModelRuntimePort, Result, RuntimeTelemetry, TransferPhase, TransferProgress,
    },
    catalog::{
        DownloadedModel, ModelVariants, Removal, SearchResults,
        TransferPhase as NativeTransferPhase, TransferUpdate as NativeTransferUpdate,
    },
    config::{ConfigPresentation, StateRecovery},
};

#[tonic::async_trait]
impl CatalogPort for NativeRuntime {
    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResults> {
        self.search_catalog(query, limit, cursor).await
    }

    async fn variants(&self, repo_id: &str) -> Result<ModelVariants> {
        self.catalog_variants(repo_id).await
    }

    async fn remove(&self, repo_id: &str) -> Result<Removal> {
        self.remove_download(repo_id).await
    }

    async fn pull(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        updates: mpsc::Sender<TransferProgress>,
        cancellation: &libmir::CancellationToken,
    ) -> Result<DownloadedModel> {
        let (native_updates, mut receiver) = mpsc::channel::<NativeTransferUpdate>(64);
        let forward_cancellation = cancellation.clone();
        let forward = tokio::spawn(async move {
            while let Some(update) = receiver.recv().await {
                let progress = TransferProgress {
                    phase: transfer_phase(update.phase),
                    downloaded_bytes: update.downloaded_bytes,
                    total_bytes: update.total_bytes,
                    message: update.message,
                };
                if updates.send(progress).await.is_err() {
                    forward_cancellation.cancel();
                    break;
                }
            }
        });
        let result = self.catalog.pull(repo_id, revision, native_updates, cancellation).await;
        forward.await.map_err(|error| Error::Infrastructure(error.to_string()))?;
        Ok(result?)
    }
}

const fn transfer_phase(phase: NativeTransferPhase) -> TransferPhase {
    match phase {
        NativeTransferPhase::Queued => TransferPhase::Queued,
        NativeTransferPhase::Checking => TransferPhase::Checking,
        NativeTransferPhase::Resolving => TransferPhase::Resolving,
        NativeTransferPhase::Downloading => TransferPhase::Downloading,
        NativeTransferPhase::Validating => TransferPhase::Validating,
        NativeTransferPhase::Available => TransferPhase::Available,
    }
}

#[tonic::async_trait]
impl ConfigurationPort for NativeRuntime {
    fn configuration(&self) -> Result<ConfigPresentation> {
        self.configuration()
    }

    async fn update_configuration(
        &self,
        change: ConfigurationChange,
    ) -> Result<ConfigurationOutcome> {
        self.update_configuration(change).await
    }
}

impl ModelRuntimePort for NativeRuntime {
    fn telemetry_source(&self) -> crate::application::TelemetrySource {
        self.telemetry_source.clone()
    }

    fn telemetry(&self) -> Result<RuntimeTelemetry> {
        self.telemetry()
    }

    fn models(&self) -> Result<Vec<ModelInfo>> {
        self.models()
    }

    fn active_models(&self) -> Result<Vec<String>> {
        self.active_models()
    }

    fn take_state_recovery(&self) -> Result<Option<StateRecovery>> {
        self.take_state_recovery()
    }

    fn local_models(&self) -> Result<Vec<LocalModelInfo>> {
        self.local_models()
    }

    fn offers(&self, selector: &str) -> Result<bool> {
        self.offers(selector)
    }

    fn inspect_model(
        &self,
        selector: &str,
        context: Option<ContextChoice>,
    ) -> Result<ModelInspection> {
        self.inspect_model(selector, context)
    }

    fn prepare_load(&self, selector: &str, defaults: LoadDefaults) -> Result<String> {
        self.prepare_load(selector, defaults)
    }

    fn save_model_defaults(&self, selector: &str, defaults: ModelDefaults) -> Result<String> {
        self.save_model_defaults(selector, defaults)
    }

    fn load_model(
        &self,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry> {
        self.load_model(selector, force, progress)
    }

    fn unload_model(&self, selector: &str) -> Result<bool> {
        self.unload_model(selector)
    }

    fn embed(&self, selector: &str, request: EmbeddingRequest) -> Result<EmbeddingOutput> {
        self.embed(selector, request)
    }

    fn rerank(&self, selector: &str, request: RerankRequest) -> Result<RerankOutput> {
        self.rerank(selector, request)
    }
}
