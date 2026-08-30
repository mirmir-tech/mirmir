use super::{Application, MemoryReport, Result};
use crate::config::{GenerationConfig, HubModelConfig};

impl Application {
    pub fn inspect_model(&self, selector: &str) -> Result<ModelInspection> {
        self.runtime.inspect_model(selector)
    }

    pub fn prepare_load(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        generation: Option<GenerationConfig>,
    ) -> Result<String> {
        self.runtime.prepare_load(selector, config_id, hub, generation)
    }

    pub fn save_generation_defaults(
        &self,
        selector: &str,
        generation: GenerationConfig,
    ) -> Result<String> {
        self.runtime.save_generation_defaults(selector, generation)
    }
}

pub struct ModelInspection {
    pub settings: Option<libmir::GenerationSettings>,
    pub has_mirmir_overrides: bool,
    pub memory: MemoryReport,
    pub task: &'static str,
    pub capabilities: ModelTaskCapabilities,
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
