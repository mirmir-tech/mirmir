use libmir::{
    GenerationOverrides, ModelDescriptor,
    models::execution::{ModelTask, PoolingMode, TaskExecutionPlan},
};
use tonic::Status;

use super::RuntimeService;
use crate::{
    config::{GenerationConfig, HubModelConfig},
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn inspect_model(
        &self,
        selector: &str,
    ) -> Result<proto::InspectModelResponse, Status> {
        let resolved = self
            .store
            .resolve_model(selector)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let has_mirmir_overrides = resolved.generation.has_overrides();
        let descriptor = ModelDescriptor::inspect(&resolved.path, overrides(resolved.generation))
            .map_err(|error| Status::failed_precondition(error.to_string()))?;
        let memory = super::preflight::report(&self.library, &descriptor)?;
        Ok(proto::InspectModelResponse {
            settings: matches!(descriptor.task(), ModelTask::Generation)
                .then(|| settings(descriptor.generation())),
            has_mirmir_overrides,
            memory: Some(memory_estimate(memory)),
            task: task_name(&descriptor.task()).to_owned(),
            capabilities: Some(capabilities(&descriptor)),
        })
    }

    pub(super) fn prepare_load(&self, request: &proto::LoadModelRequest) -> Result<String, Status> {
        let Some(settings) = request.settings.as_ref() else {
            return Ok(request.selector.clone());
        };
        let generation = config(settings)?;
        let resolved = self
            .store
            .resolve_model(&request.selector)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        ModelDescriptor::inspect(&resolved.path, overrides(generation))
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let hub = (!request.repo_id.is_empty()).then(|| HubModelConfig {
            repo_id: request.repo_id.clone(),
            revision: request.revision.clone(),
            commit: request.commit.clone(),
        });
        self.store
            .save_model_generation(&request.selector, &request.config_id, hub, generation)
            .map_err(|error| Status::invalid_argument(error.to_string()))
    }

    pub(super) fn save_generation_defaults(
        &self,
        selector: &str,
        settings: &proto::GenerationSettings,
    ) -> Result<String, Status> {
        let generation = config(settings)?;
        let resolved = self
            .store
            .resolve_model(selector)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        ModelDescriptor::inspect(&resolved.path, overrides(generation))
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        self.store
            .save_model_generation(selector, &resolved.key, None, generation)
            .map_err(|error| Status::invalid_argument(error.to_string()))
    }
}

fn capabilities(descriptor: &ModelDescriptor) -> proto::ModelTaskCapabilities {
    let max_input_tokens = descriptor
        .tokenizer()
        .default_max_length()
        .unwrap_or_else(|| descriptor.metadata().context_len)
        .min(descriptor.metadata().context_len);
    let (embedding, rerank) = match descriptor.task_plan() {
        TaskExecutionPlan::Embedding { task, .. } => (
            Some(proto::EmbeddingCapabilities {
                native_dimensions: u64::try_from(task.native_dimensions).unwrap_or(u64::MAX),
                pooling: pooling(task.pooling).to_owned(),
                normalized: task.normalize,
                prompt_names: task.prompts.keys().cloned().collect(),
                default_prompt: task.default_prompt.clone(),
                includes_prompt: task.include_prompt,
            }),
            None,
        ),
        TaskExecutionPlan::SequenceScoring { task, .. } => (
            None,
            Some(proto::RerankCapabilities {
                labels: u64::try_from(task.labels).unwrap_or(u64::MAX),
                pooling: pooling(task.pooling).to_owned(),
                raw_scores: true,
            }),
        ),
        TaskExecutionPlan::Generation { .. } => (None, None),
    };
    proto::ModelTaskCapabilities {
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

fn memory_estimate(value: super::preflight::MemoryReport) -> proto::ModelMemoryEstimate {
    proto::ModelMemoryEstimate {
        weight_bytes: value.estimate.weight_bytes,
        kv_cache_bytes: value.estimate.kv_cache_bytes,
        workspace_bytes: value.estimate.workspace_bytes,
        required_bytes: value.estimate.required_bytes,
        available_bytes: value.available,
        budget_bytes: value.budget,
        memory_source: value.source,
        fit: value.fit.to_owned(),
        max_safe_context_tokens: value.max_safe_context,
        configured_cache_tokens: value.estimate.cache_capacity_tokens,
    }
}

const fn overrides(config: GenerationConfig) -> GenerationOverrides {
    GenerationOverrides {
        max_tokens: config.max_tokens,
        temperature: config.temperature,
        top_p: config.top_p,
        top_k: config.top_k,
        repetition_penalty: config.repetition_penalty,
    }
}

fn settings(value: libmir::GenerationSettings) -> proto::GenerationSettings {
    proto::GenerationSettings {
        max_tokens: u64::try_from(value.max_tokens).unwrap_or(u64::MAX),
        temperature: value.temperature,
        top_p: value.top_p,
        top_k: u64::try_from(value.top_k).unwrap_or(u64::MAX),
        repetition_penalty: value.repetition_penalty,
    }
}

fn config(value: &proto::GenerationSettings) -> Result<GenerationConfig, Status> {
    Ok(GenerationConfig {
        max_tokens: Some(usize::try_from(value.max_tokens).map_err(integer)?),
        temperature: Some(value.temperature),
        top_p: Some(value.top_p),
        top_k: Some(usize::try_from(value.top_k).map_err(integer)?),
        repetition_penalty: Some(value.repetition_penalty),
    })
}

fn integer(error: std::num::TryFromIntError) -> Status {
    Status::invalid_argument(error.to_string())
}
