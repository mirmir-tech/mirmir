use libmir::{EmbeddingOutput, EmbeddingRequest, ProgressEvent, RerankOutput, RerankRequest};

use crate::{
    application::{
        LocalModelInfo, ModelInfo, ModelInspection, Result, models::ModelEntry,
        telemetry::RuntimeTelemetry,
    },
    config::{GenerationConfig, HubModelConfig, StateRecovery},
};

pub trait ModelRuntimePort: Send + Sync {
    fn telemetry(&self) -> Result<RuntimeTelemetry>;
    fn models(&self) -> Result<Vec<ModelInfo>>;
    fn active_models(&self) -> Result<Vec<String>>;
    fn take_state_recovery(&self) -> Result<Option<StateRecovery>>;
    fn local_models(&self) -> Result<Vec<LocalModelInfo>>;
    fn inspect_model(&self, selector: &str) -> Result<ModelInspection>;
    fn prepare_load(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        generation: Option<GenerationConfig>,
    ) -> Result<String>;
    fn save_generation_defaults(
        &self,
        selector: &str,
        generation: GenerationConfig,
    ) -> Result<String>;
    fn load_model(
        &self,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry>;
    fn unload_model(&self, selector: &str) -> Result<bool>;
    fn embed(&self, selector: &str, request: EmbeddingRequest) -> Result<EmbeddingOutput>;
    fn rerank(&self, selector: &str, request: RerankRequest) -> Result<RerankOutput>;
}
