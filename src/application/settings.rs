use libmir::{
    GenerationOverrides, ModelDescriptor,
    models::execution::{ModelTask, PoolingMode, TaskExecutionPlan},
};

use super::{MemoryReport, Result, RuntimeCoordinator};
use crate::config::{GenerationConfig, HubModelConfig};

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

impl RuntimeCoordinator {
    pub fn inspect_model(&self, selector: &str) -> Result<ModelInspection> {
        let resolved = self.store.resolve_model(selector)?;
        let descriptor = ModelDescriptor::inspect(&resolved.path, overrides(resolved.generation))?;
        let memory = self.memory_report(&descriptor)?;
        Ok(ModelInspection {
            settings: matches!(descriptor.task(), ModelTask::Generation)
                .then(|| descriptor.generation()),
            has_mirmir_overrides: resolved.generation.has_overrides(),
            memory,
            task: task_name(&descriptor.task()),
            capabilities: capabilities(&descriptor),
        })
    }

    pub fn prepare_load(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        generation: Option<GenerationConfig>,
    ) -> Result<String> {
        let Some(generation) = generation else {
            return Ok(selector.to_owned());
        };
        let resolved = self.store.resolve_model(selector)?;
        ModelDescriptor::inspect(&resolved.path, overrides(generation))?;
        Ok(self.store.save_model_generation(selector, config_id, hub, generation)?)
    }

    pub fn save_generation_defaults(
        &self,
        selector: &str,
        generation: GenerationConfig,
    ) -> Result<String> {
        let resolved = self.store.resolve_model(selector)?;
        ModelDescriptor::inspect(&resolved.path, overrides(generation))?;
        Ok(self.store.save_model_generation(selector, &resolved.key, None, generation)?)
    }
}

fn capabilities(descriptor: &ModelDescriptor) -> ModelTaskCapabilities {
    let max_input_tokens = descriptor
        .tokenizer()
        .default_max_length()
        .unwrap_or_else(|| descriptor.metadata().context_len)
        .min(descriptor.metadata().context_len);
    let (embedding, rerank) = match descriptor.task_plan() {
        TaskExecutionPlan::Embedding { task, .. } => (
            Some(EmbeddingCapabilities {
                native_dimensions: u64::try_from(task.native_dimensions).unwrap_or(u64::MAX),
                pooling: pooling(task.pooling),
                normalized: task.normalize,
                prompt_names: task.prompts.keys().cloned().collect(),
                default_prompt: task.default_prompt.clone(),
                includes_prompt: task.include_prompt,
            }),
            None,
        ),
        TaskExecutionPlan::SequenceScoring { task, .. } => (
            None,
            Some(RerankCapabilities {
                labels: u64::try_from(task.labels).unwrap_or(u64::MAX),
                pooling: pooling(task.pooling),
                raw_scores: true,
            }),
        ),
        TaskExecutionPlan::Generation { .. } => (None, None),
    };
    ModelTaskCapabilities {
        max_input_tokens: u64::try_from(max_input_tokens).unwrap_or(u64::MAX),
        embedding,
        rerank,
    }
}

const fn pooling(mode: PoolingMode) -> &'static str {
    match mode {
        PoolingMode::Cls => "cls",
        PoolingMode::LastToken => "last_token",
        PoolingMode::Mean => "mean",
    }
}

const fn task_name(task: &ModelTask) -> &'static str {
    match task {
        ModelTask::Generation => "generation",
        ModelTask::Embedding(_) => "embedding",
        ModelTask::SequenceScoring(_) => "rerank",
    }
}

const fn overrides(config: GenerationConfig) -> GenerationOverrides {
    GenerationOverrides {
        max_tokens: config.max_tokens,
        min_tokens: None,
        ignore_eos: None,
        temperature: config.temperature,
        top_p: config.top_p,
        top_k: config.top_k,
        repetition_penalty: config.repetition_penalty,
    }
}
