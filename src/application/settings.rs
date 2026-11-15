use super::{Application, ContextChoice, Error, MemoryReport, ModelContext, Result};
use crate::config::{GenerationConfig, HubModelConfig};

impl Application {
    /// Refuses a selector a remote caller may not name with the error an
    /// unknown model gets, so the refusal reveals nothing about the path.
    pub fn check_offered(&self, selector: &str) -> Result<()> {
        if self.runtime.offers(selector)? {
            Ok(())
        } else {
            Err(Error::InvalidModel(selector.to_owned()))
        }
    }

    /// Inspects a model; `context` estimates memory for that context instead
    /// of the one saved for the model.
    pub fn inspect_model(
        &self,
        selector: &str,
        context: Option<ContextChoice>,
    ) -> Result<ModelInspection> {
        self.runtime.inspect_model(selector, context)
    }

    pub fn prepare_load(&self, selector: &str, defaults: LoadDefaults) -> Result<String> {
        self.runtime.prepare_load(selector, defaults)
    }

    /// Saves a model's generation defaults and context for its next load.
    pub fn save_model_defaults(&self, selector: &str, defaults: ModelDefaults) -> Result<String> {
        self.runtime.save_model_defaults(selector, defaults)
    }
}

pub struct ModelInspection {
    pub settings: Option<libmir::GenerationSettings>,
    pub has_mirmir_overrides: bool,
    pub memory: MemoryReport,
    pub task: &'static str,
    pub capabilities: ModelTaskCapabilities,
    /// `None` for models without a K/V cache, such as embedding models.
    pub context: Option<ModelContext>,
}

/// What a client saves for a model, each part only when it changes it.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModelDefaults {
    pub generation: Option<GenerationConfig>,
    pub context: Option<ContextChoice>,
}

/// What a load saves for its model first, and where a direct path is kept.
#[derive(Clone, Debug, Default)]
pub struct LoadDefaults {
    pub config_id: String,
    pub hub: Option<HubModelConfig>,
    pub defaults: ModelDefaults,
}

pub struct ModelTaskCapabilities {
    pub max_input_tokens: u64,
    pub embedding: Option<EmbeddingCapabilities>,
    pub rerank: Option<RerankCapabilities>,
}

pub struct EmbeddingCapabilities {
    pub native_dimensions: u64,
    pub pooling: &'static str,
    pub normalized: bool,
    pub prompt_names: Vec<String>,
    pub default_prompt: Option<String>,
    pub includes_prompt: bool,
}

pub struct RerankCapabilities {
    pub labels: u64,
    pub pooling: &'static str,
    pub raw_scores: bool,
}
