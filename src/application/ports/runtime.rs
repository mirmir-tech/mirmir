use libmir::{EmbeddingOutput, EmbeddingRequest, ProgressEvent, RerankOutput, RerankRequest};

use crate::{
    application::{
        ContextChoice, LoadDefaults, LocalModelInfo, ModelDefaults, ModelInfo, ModelInspection,
        Result, models::ModelEntry, telemetry::RuntimeTelemetry,
    },
    config::StateRecovery,
};

pub trait ModelRuntimePort: Send + Sync {
    fn telemetry_source(&self) -> crate::application::TelemetrySource {
        crate::application::TelemetrySource::default()
    }
    fn telemetry(&self) -> Result<RuntimeTelemetry>;
    fn models(&self) -> Result<Vec<ModelInfo>>;
    fn active_models(&self) -> Result<Vec<String>>;
    fn take_state_recovery(&self) -> Result<Option<StateRecovery>>;
    fn local_models(&self) -> Result<Vec<LocalModelInfo>>;
    /// Whether a remote caller may name `selector`: a configured key, or a
    /// model path the server lists itself.
    fn offers(&self, selector: &str) -> Result<bool>;
    fn inspect_model(
        &self,
        selector: &str,
        context: Option<ContextChoice>,
    ) -> Result<ModelInspection>;
    fn prepare_load(&self, selector: &str, defaults: LoadDefaults) -> Result<String>;
    fn save_model_defaults(&self, selector: &str, defaults: ModelDefaults) -> Result<String>;
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
