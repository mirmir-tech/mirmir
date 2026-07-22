use super::{Activity, CatalogModel, CatalogResults, Features, Model, Overview};
use crate::rpc::proto;

impl From<proto::TelemetrySnapshot> for Overview {
    fn from(snapshot: proto::TelemetrySnapshot) -> Self {
        Self {
            sampled_at_unix_ms: snapshot.sampled_at_unix_ms,
            uptime_ms: snapshot.uptime_ms,
            loaded_models: snapshot.loaded_models,
            active_requests: snapshot.active_requests,
            total_requests: snapshot.total_requests,
            completed_requests: snapshot.completed_requests,
            failed_requests: snapshot.failed_requests,
            prompt_tokens: snapshot.prompt_tokens,
            completion_tokens: snapshot.completion_tokens,
            current_tokens_per_second: snapshot.current_tokens_per_second,
            current_prefill_tokens_per_second: snapshot.current_prefill_tokens_per_second,
            current_decode_tokens_per_second: snapshot.current_decode_tokens_per_second,
            current_ttft_ms: snapshot.current_ttft_ms,
            last_tokens_per_second: snapshot.last_tokens_per_second,
            mean_tokens_per_second: snapshot.mean_tokens_per_second,
            last_prefill_tokens_per_second: snapshot.last_prefill_tokens_per_second,
            mean_prefill_tokens_per_second: snapshot.mean_prefill_tokens_per_second,
            last_decode_tokens_per_second: snapshot.last_decode_tokens_per_second,
            mean_decode_tokens_per_second: snapshot.mean_decode_tokens_per_second,
            last_ttft_ms: snapshot.last_ttft_ms,
            mean_ttft_ms: snapshot.mean_ttft_ms,
            host_total_memory_bytes: snapshot.host_total_memory_bytes,
            host_available_memory_bytes: snapshot.host_available_memory_bytes,
            memory_source: snapshot.memory_source,
            kv_total_blocks: snapshot.kv_total_blocks,
            kv_used_blocks: snapshot.kv_used_blocks,
            kv_cached_prefixes: snapshot.kv_cached_prefixes,
            kv_hit_tokens: snapshot.kv_hit_tokens,
            kv_miss_tokens: snapshot.kv_miss_tokens,
            active_elapsed_ms: snapshot.active_elapsed_ms,
            active_prompt_tokens: snapshot.active_prompt_tokens,
            active_completion_tokens: snapshot.active_completion_tokens,
            active_stage: snapshot.active_stage,
        }
    }
}

impl From<proto::LocalModelInfo> for Model {
    fn from(model: proto::LocalModelInfo) -> Self {
        Self {
            id: model.id,
            repo_id: model.repo_id,
            revision: model.revision,
            commit: model.commit,
            path: model.path,
            state: model.state,
            selector: model.selector,
            managed: model.managed,
            image_input: model.image_input,
            image_unavailable_reason: model.image_unavailable_reason,
            loadable: model.loadable,
            load_unavailable_reason: model.load_unavailable_reason,
            class: model.model_class,
            library: model.library,
            ecosystem: model.ecosystem,
            container: model.container,
            encoding: model.encoding,
            metal_compatibility: model.metal_compatibility,
            cuda_compatibility: model.cuda_compatibility,
            size_bytes: model.size_bytes,
            features: features(model.tool_use, model.thinking, model.vision),
        }
    }
}

impl From<proto::SearchModelsResponse> for CatalogResults {
    fn from(response: proto::SearchModelsResponse) -> Self {
        Self {
            models: response.models.into_iter().map(Into::into).collect(),
            total_memory_bytes: response.total_memory_bytes,
            available_memory_bytes: response.available_memory_bytes,
            memory_source: response.memory_source,
            next_cursor: response.next_cursor,
        }
    }
}

impl From<proto::CatalogModel> for CatalogModel {
    fn from(model: proto::CatalogModel) -> Self {
        Self {
            id: model.id,
            downloads: model.downloads,
            likes: model.likes,
            gated: model.gated,
            model_class: model.model_class,
            compatibility: model.compatibility,
            memory_fit: model.memory_fit,
            estimated_required_bytes: model.estimated_required_bytes,
            budget_bytes: model.budget_bytes,
            confidence: model.confidence,
            reason: model.reason,
            downloaded: model.downloaded,
            local_source: model.local_source,
            estimated_weight_bytes: model.estimated_weight_bytes,
            library: model.library,
            ecosystem: model.ecosystem,
            container: model.container,
            encoding: model.encoding,
            metal_compatibility: model.metal_compatibility,
            cuda_compatibility: model.cuda_compatibility,
            features: features(model.tool_use, model.thinking, model.vision),
        }
    }
}

impl From<proto::ActivityEvent> for Activity {
    fn from(event: proto::ActivityEvent) -> Self {
        Self {
            operation_id: event.operation_id,
            kind: event.kind,
            target: event.target,
            state: event.state,
            stage: event.stage,
            detail: event.detail,
            updated_at_unix_ms: event.updated_at_unix_ms,
            cancellable: event.cancellable,
            current: event.current,
            total: event.total,
        }
    }
}

const fn features(tool_use: bool, thinking: bool, vision: bool) -> Features {
    Features { tool_use, thinking, vision }
}
