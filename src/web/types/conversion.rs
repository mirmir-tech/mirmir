use super::{
    Activity, CatalogModel, CatalogResults, CatalogVariant, Features, Model, ModelVariants,
    Overview,
};
use crate::{
    application, catalog,
    model::format::{container_name, legacy_library},
};

impl From<application::TelemetrySnapshot> for Overview {
    fn from(snapshot: application::TelemetrySnapshot) -> Self {
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
            host_total_memory_bytes: snapshot.host_memory.map(|memory| memory.total_bytes),
            host_available_memory_bytes: snapshot.host_memory.map(|memory| memory.available_bytes),
            memory_source: application::HostMemory::LABEL.to_owned(),
            kv_total_blocks: snapshot.kv_total_blocks,
            kv_used_blocks: snapshot.kv_used_blocks,
            kv_cached_prefixes: snapshot.kv_cached_prefixes,
            kv_hit_tokens: snapshot.kv_hit_tokens,
            kv_miss_tokens: snapshot.kv_miss_tokens,
            active_elapsed_ms: snapshot.active_elapsed_ms,
            active_prompt_tokens: snapshot.active_prompt_tokens,
            active_completion_tokens: snapshot.active_completion_tokens,
            active_stage: snapshot.active_stage,
            gpu_utilization_percent: snapshot.gpu_utilization_percent,
            device_temperature_celsius: snapshot.device_temperature_celsius,
            device_power_watts: snapshot.device_power_watts,
            device_power_limit_watts: snapshot.device_power_limit_watts,
            device_name: snapshot.device_name,
        }
    }
}

impl From<application::LocalModelInfo> for Model {
    fn from(model: application::LocalModelInfo) -> Self {
        Self {
            id: model.id,
            repo_id: model.repo_id,
            revision: model.revision,
            commit: model.commit,
            path: model.path,
            state: model.state.as_str().to_owned(),
            selector: model.selector,
            managed: model.managed,
            image_input: model.image_input,
            image_unavailable_reason: model.image_unavailable_reason,
            loadable: model.loadable,
            load_unavailable_reason: model.load_unavailable_reason,
            class: model.model_class,
            library: legacy_library(model.container, &model.ecosystem).to_owned(),
            ecosystem: model.ecosystem.name().to_owned(),
            container: container_name(model.container).to_owned(),
            encoding: model.encoding.label(),
            metal_compatibility: model.metal_compatibility.as_str().to_owned(),
            cuda_compatibility: model.cuda_compatibility.as_str().to_owned(),
            size_bytes: model.size_bytes,
            features: features(model.tool_use, model.thinking, model.vision),
        }
    }
}

impl From<catalog::SearchResults> for CatalogResults {
    fn from(response: catalog::SearchResults) -> Self {
        Self {
            models: response.models.into_iter().map(Into::into).collect(),
            total_memory_bytes: response.memory.total,
            available_memory_bytes: response.memory.available,
            memory_source: response.memory.source.to_owned(),
            next_cursor: response.next_cursor,
        }
    }
}

impl From<catalog::CatalogModel> for CatalogModel {
    fn from(model: catalog::CatalogModel) -> Self {
        Self {
            id: model.id,
            downloads: model.downloads,
            likes: model.likes,
            gated: model.gated,
            model_class: model.model_class,
            compatibility: model.compatibility.as_str().to_owned(),
            memory_fit: model.memory_fit.as_str().to_owned(),
            estimated_required_bytes: model.required_bytes,
            budget_bytes: model.budget_bytes,
            confidence: model.confidence.as_str().to_owned(),
            reason: model.reason,
            downloaded: model.downloaded,
            local_source: model.local_source.to_owned(),
            estimated_weight_bytes: model.weight_bytes,
            library: legacy_library(model.container, &model.ecosystem).to_owned(),
            ecosystem: model.ecosystem.name().to_owned(),
            container: container_name(model.container).to_owned(),
            encoding: model.encoding.label(),
            metal_compatibility: model.metal_compatibility.as_str().to_owned(),
            cuda_compatibility: model.cuda_compatibility.as_str().to_owned(),
            preflight_bytes: model.preflight_bytes,
            preflight_error: model.preflight_error,
            features: features(
                model.features.tool_use,
                model.features.thinking,
                model.features.vision,
            ),
            variants: model.variants.into_iter().map(CatalogVariant::from).collect(),
            default_variant: model.default_variant,
        }
    }
}

impl From<catalog::CatalogVariant> for CatalogVariant {
    fn from(variant: catalog::CatalogVariant) -> Self {
        Self {
            name: variant.name,
            size_kind: variant.size.kind().to_owned(),
            weight_bytes: variant.size.bytes(),
            required_bytes: variant.required_bytes,
            memory_fit: variant.memory_fit.as_str().to_owned(),
            downloaded: variant.downloaded,
        }
    }
}

impl From<catalog::ModelVariants> for ModelVariants {
    fn from(listed: catalog::ModelVariants) -> Self {
        Self {
            variants: listed.variants.into_iter().map(CatalogVariant::from).collect(),
            default_variant: listed.default_variant,
        }
    }
}

impl From<application::ActivityEvent> for Activity {
    fn from(event: application::ActivityEvent) -> Self {
        let progress = event.status.progress();
        Self {
            operation_id: event.operation_id,
            kind: event.kind.as_str().to_owned(),
            target: event.target,
            state: event.status.state_str().to_owned(),
            stage: event.status.stage_str().to_owned(),
            detail: event.detail,
            updated_at_unix_ms: event.updated_at_unix_ms,
            cancellable: event.cancellable,
            current: progress.map(application::ActivityProgress::current),
            total: progress.and_then(application::ActivityProgress::total),
        }
    }
}

const fn features(tool_use: bool, thinking: bool, vision: bool) -> Features {
    Features { tool_use, thinking, vision }
}
