mod context;

use tonic::Status;

use self::context::{choice, model_context};
use super::RuntimeService;
use crate::{
    application::{LoadDefaults, ModelDefaults, ModelInspection, ModelTaskCapabilities},
    config::{GenerationConfig, HubModelConfig},
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn inspect_model(
        &self,
        request: &proto::InspectModelRequest,
    ) -> Result<proto::InspectModelResponse, Status> {
        let context = request.context.as_ref().map(choice).transpose()?;
        self.application
            .inspect_model(&request.selector, context)
            .map(inspection)
            .map_err(super::status::application)
    }

    pub(super) fn prepare_load(&self, request: &proto::LoadModelRequest) -> Result<String, Status> {
        let hub = (!request.repo_id.is_empty()).then(|| {
            HubModelConfig::new(
                request.repo_id.clone(),
                request.revision.clone(),
                request.commit.clone(),
            )
        });
        let defaults = ModelDefaults {
            generation: request.settings.as_ref().map(config).transpose()?,
            context: request.context.as_ref().map(choice).transpose()?,
        };
        let load = LoadDefaults {
            config_id: request.config_id.clone(),
            hub,
            defaults,
        };
        self.application
            .prepare_load(&request.selector, load)
            .map_err(super::status::application)
    }

    /// Saves on the blocking pool: a first configuration waits for the
    /// memory gate a running load holds, and that load's progress stream
    /// needs the async workers to drain.
    pub(super) async fn save_model_defaults(
        self,
        request: proto::UpdateModelGenerationRequest,
    ) -> Result<String, Status> {
        super::status::internal(
            tokio::task::spawn_blocking(move || self.save_defaults_now(&request)).await,
        )?
    }

    fn save_defaults_now(
        &self,
        request: &proto::UpdateModelGenerationRequest,
    ) -> Result<String, Status> {
        let defaults = ModelDefaults {
            generation: request.settings.as_ref().map(config).transpose()?,
            context: request.context.as_ref().map(choice).transpose()?,
        };
        if defaults.generation.is_none() && defaults.context.is_none() {
            return Err(Status::invalid_argument("generation settings or a context are required"));
        }
        self.application
            .save_model_defaults(&request.selector, defaults)
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
        context: value.context.map(model_context),
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
