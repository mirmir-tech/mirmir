use libmir::{EmbeddingOutput, EmbeddingRequest, ProgressEvent, RerankOutput, RerankRequest};
use tokio::sync::mpsc;

use super::NativeRuntime;
use crate::{
    application::{
        CatalogPort, ConfigurationChange, ConfigurationOutcome, ConfigurationPort, LocalModelInfo,
        ModelEntry, ModelInfo, ModelInspection, ModelRuntimePort, Result, RuntimeTelemetry,
    },
    catalog::{DownloadedModel, Removal, SearchResults, TransferUpdate},
    config::{ConfigPresentation, GenerationConfig, HubModelConfig, StateRecovery},
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

    async fn remove(&self, repo_id: &str) -> Result<Removal> {
        self.remove_download(repo_id).await
    }

    async fn pull(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        updates: mpsc::Sender<TransferUpdate>,
        cancellation: &libmir::CancellationToken,
    ) -> Result<DownloadedModel> {
        Ok(self.catalog.pull(repo_id, revision, updates, cancellation).await?)
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

    fn inspect_model(&self, selector: &str) -> Result<ModelInspection> {
        self.inspect_model(selector)
    }

    fn prepare_load(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        generation: Option<GenerationConfig>,
    ) -> Result<String> {
        self.prepare_load(selector, config_id, hub, generation)
    }

    fn save_generation_defaults(
        &self,
        selector: &str,
        generation: GenerationConfig,
    ) -> Result<String> {
        self.save_generation_defaults(selector, generation)
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
