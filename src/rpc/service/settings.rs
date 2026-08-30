use tonic::Status;

use super::RuntimeService;
use crate::{
    application::{ModelInspection, ModelTaskCapabilities},
    config::{GenerationConfig, HubModelConfig},
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn inspect_model(
        &self,
        selector: &str,
    ) -> Result<proto::InspectModelResponse, Status> {
        self.application
            .inspect_model(selector)
            .map(inspection)
            .map_err(super::status::application)
    }

    pub(super) fn prepare_load(&self, request: &proto::LoadModelRequest) -> Result<String, Status> {
        let generation = request.settings.as_ref().map(config).transpose()?;
        let hub = (!request.repo_id.is_empty()).then(|| HubModelConfig {
            repo_id: request.repo_id.clone(),
            revision: request.revision.clone(),
            commit: request.commit.clone(),
        });
        self.application
            .prepare_load(&request.selector, &request.config_id, hub, generation)
            .map_err(super::status::application)
    }

    pub(super) fn save_generation_defaults(
        &self,
        selector: &str,
        settings: &proto::GenerationSettings,
    ) -> Result<String, Status> {
        self.application
            .save_generation_defaults(selector, config(settings)?)
            .map_err(super::status::application)
    }
}

fn inspection(value: ModelInspection) -> proto::InspectModelResponse {
    proto::InspectModelResponse {
        settings: value.settings.map(settings),
        has_mirmir_overrides: value.has_mirmir_overrides,
        memory: Some(memory_estimate(value.memory)),
        task: value.task.to_owned(),
        capabilities: Some(capabilities(value.capabilities)),
    }
}

fn capabilities(value: ModelTaskCapabilities) -> proto::ModelTaskCapabilities {
    proto::ModelTaskCapabilities {
        max_input_tokens: value.max_input_tokens,
        embedding: value.embedding.map(|item| proto::EmbeddingCapabilities {
            native_dimensions: item.native_dimensions,
            pooling: item.pooling.to_owned(),
            normalized: item.normalized,
            prompt_names: item.prompt_names,
            default_prompt: item.default_prompt,
            includes_prompt: item.includes_prompt,
        }),
        rerank: value.rerank.map(|item| proto::RerankCapabilities {
            labels: item.labels,
            pooling: item.pooling.to_owned(),
            raw_scores: item.raw_scores,
        }),
    }
}

fn memory_estimate(value: crate::application::MemoryReport) -> proto::ModelMemoryEstimate {
    proto::ModelMemoryEstimate {
        weight_bytes: value.estimate.weight_bytes,
        kv_cache_bytes: value.estimate.kv_cache_bytes,
        workspace_bytes: value.estimate.workspace_bytes,
        required_bytes: value.estimate.required_bytes,
        available_bytes: value.available,
        budget_bytes: value.budget,
        memory_source: value.source,
        fit: value.fit.as_str().to_owned(),
        max_safe_context_tokens: value.max_safe_context,
        configured_cache_tokens: value.estimate.cache_capacity_tokens,
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
        max_tokens: Some(super::status::invalid(usize::try_from(value.max_tokens))?),
        temperature: Some(value.temperature),
        top_p: Some(value.top_p),
        top_k: Some(super::status::invalid(usize::try_from(value.top_k))?),
        repetition_penalty: Some(value.repetition_penalty),
    })
}
