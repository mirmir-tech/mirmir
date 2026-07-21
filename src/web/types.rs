use serde::Serialize;

use crate::rpc::proto;

#[derive(Serialize)]
pub struct Overview {
    pub sampled_at_unix_ms: u64,
    pub uptime_ms: u64,
    pub loaded_models: u64,
    pub active_requests: u64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub current_tokens_per_second: Option<f64>,
    pub current_prefill_tokens_per_second: Option<f64>,
    pub current_decode_tokens_per_second: Option<f64>,
    pub current_ttft_ms: Option<f64>,
    pub last_tokens_per_second: Option<f64>,
    pub last_ttft_ms: Option<f64>,
    pub host_total_memory_bytes: Option<u64>,
    pub host_available_memory_bytes: Option<u64>,
    pub memory_source: String,
    pub kv_total_blocks: u64,
    pub kv_used_blocks: u64,
    pub active_stage: String,
}

#[derive(Serialize)]
pub struct Models {
    pub models: Vec<Model>,
}

#[derive(Serialize)]
pub struct Model {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub path: String,
    pub state: String,
    pub selector: String,
    pub managed: bool,
    pub image_input: bool,
    pub image_unavailable_reason: String,
    pub loadable: bool,
    pub load_unavailable_reason: String,
    #[serde(rename = "model_class")]
    pub class: String,
}

#[derive(Serialize)]
pub struct CatalogResults {
    pub models: Vec<CatalogModel>,
    pub total_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub memory_source: String,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct CatalogModel {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub gated: bool,
    pub model_class: String,
    pub compatibility: String,
    pub memory_fit: String,
    pub estimated_required_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub confidence: String,
    pub reason: String,
    pub downloaded: bool,
    pub local_source: String,
}

#[derive(Serialize)]
pub struct Activity {
    pub operation_id: String,
    pub kind: String,
    pub target: String,
    pub state: String,
    pub stage: String,
    pub detail: String,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

impl From<proto::TelemetrySnapshot> for Overview {
    fn from(snapshot: proto::TelemetrySnapshot) -> Self {
        Self {
            sampled_at_unix_ms: snapshot.sampled_at_unix_ms,
            uptime_ms: snapshot.uptime_ms,
            loaded_models: snapshot.loaded_models,
            active_requests: snapshot.active_requests,
            total_requests: snapshot.total_requests,
            failed_requests: snapshot.failed_requests,
            prompt_tokens: snapshot.prompt_tokens,
            completion_tokens: snapshot.completion_tokens,
            current_tokens_per_second: snapshot.current_tokens_per_second,
            current_prefill_tokens_per_second: snapshot.current_prefill_tokens_per_second,
            current_decode_tokens_per_second: snapshot.current_decode_tokens_per_second,
            current_ttft_ms: snapshot.current_ttft_ms,
            last_tokens_per_second: snapshot.last_tokens_per_second,
            last_ttft_ms: snapshot.last_ttft_ms,
            host_total_memory_bytes: snapshot.host_total_memory_bytes,
            host_available_memory_bytes: snapshot.host_available_memory_bytes,
            memory_source: snapshot.memory_source,
            kv_total_blocks: snapshot.kv_total_blocks,
            kv_used_blocks: snapshot.kv_used_blocks,
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
