use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use super::{capabilities::TaskCapabilities, security_headers, session::WebError};
use crate::{
    application,
    config::{GenerationConfig, HubModelConfig},
    http::ApiState,
};

#[derive(Deserialize)]
pub struct InspectQuery {
    selector: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Settings {
    max_tokens: u64,
    temperature: f32,
    top_p: f32,
    top_k: u64,
    repetition_penalty: f32,
}
#[derive(Deserialize)]
pub struct LoadRequest {
    selector: String,
    settings: Option<Settings>,
    #[serde(default)]
    config_id: String,
    #[serde(default)]
    repo_id: String,
    #[serde(default)]
    revision: String,
    #[serde(default)]
    commit: String,
    #[serde(default)]
    force: bool,
}
#[derive(Deserialize)]
pub struct SelectorRequest {
    selector: String,
}
#[derive(Deserialize)]
pub struct RemoveRequest {
    repo_id: String,
}
#[derive(Deserialize)]
pub struct CancelRequest {
    operation_id: String,
}
#[derive(Serialize)]
struct Inspection {
    settings: Option<Settings>,
    has_mirmir_overrides: bool,
    memory: Memory,
    task: String,
    capabilities: Option<TaskCapabilities>,
}
#[derive(Serialize)]
struct Memory {
    required_bytes: u64,
    available_bytes: Option<u64>,
    budget_bytes: Option<u64>,
    #[serde(rename = "memory_source")]
    source: String,
    fit: String,
    max_safe_context_tokens: Option<u64>,
    configured_cache_tokens: u64,
}
#[derive(Serialize)]
struct Accepted {
    accepted: bool,
    operation: &'static str,
}
#[derive(Serialize)]
struct Unloaded {
    unloaded: bool,
}
#[derive(Serialize)]
struct Removed {
    removed: bool,
    freed_bytes: u64,
}
#[derive(Serialize)]
struct Cancelled {
    found: bool,
    accepted: bool,
    state: String,
}
pub async fn inspect(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<InspectQuery>,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let inspected = state
        .application()
        .inspect_model(&query.selector)
        .map_err(WebError::application)?;
    let response = Inspection {
        settings: inspected.settings.map(Settings::from),
        has_mirmir_overrides: inspected.has_mirmir_overrides,
        memory: Memory::from(inspected.memory),
        task: inspected.task.to_owned(),
        capabilities: Some(TaskCapabilities::from(inspected.capabilities)),
    };
    Ok((security_headers(), Json(response)).into_response())
}

pub async fn load(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<LoadRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let generation = request.settings.map(Into::into);
    let hub = (!request.repo_id.is_empty()).then_some(HubModelConfig {
        repo_id: request.repo_id,
        revision: request.revision,
        commit: request.commit,
    });
    let selector = state
        .application()
        .prepare_load(&request.selector, &request.config_id, hub, generation)
        .map_err(WebError::application)?;
    let application = state.application().clone();
    drop(tokio::task::spawn_blocking(move || {
        drop(application.load_model(&selector, request.force, &mut |_| {}));
    }));
    Ok(accepted("load"))
}

pub async fn unload(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<SelectorRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let unloaded = state
        .application()
        .unload_model(&request.selector)
        .map_err(WebError::application)?;
    Ok((security_headers(), Json(Unloaded { unloaded })).into_response())
}

pub async fn remove(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<RemoveRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let removed = state
        .application()
        .remove_download(&request.repo_id)
        .await
        .map_err(WebError::application)?;
    Ok((
        security_headers(),
        Json(Removed {
            removed: removed.removed,
            freed_bytes: removed.freed_bytes,
        }),
    )
        .into_response())
}

pub async fn cancel(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<CancelRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let cancelled = state.application().cancel_operation(&request.operation_id);
    let (found, accepted, status) = match cancelled {
        application::CancelOutcome::NotFound => (false, false, "not_found"),
        application::CancelOutcome::NotCancellable(status) => (true, false, status.state_str()),
        application::CancelOutcome::Requested => (true, true, "cancelling"),
    };
    Ok((
        security_headers(),
        Json(Cancelled {
            found,
            accepted,
            state: status.to_owned(),
        }),
    )
        .into_response())
}

fn accepted(operation: &'static str) -> Response {
    (security_headers(), Json(Accepted { accepted: true, operation })).into_response()
}

impl From<libmir::GenerationSettings> for Settings {
    fn from(settings: libmir::GenerationSettings) -> Self {
        Self {
            max_tokens: u64::try_from(settings.max_tokens).unwrap_or(u64::MAX),
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: u64::try_from(settings.top_k).unwrap_or(u64::MAX),
            repetition_penalty: settings.repetition_penalty,
        }
    }
}

impl From<Settings> for GenerationConfig {
    fn from(settings: Settings) -> Self {
        Self {
            max_tokens: usize::try_from(settings.max_tokens).ok(),
            temperature: Some(settings.temperature),
            top_p: Some(settings.top_p),
            top_k: usize::try_from(settings.top_k).ok(),
            repetition_penalty: Some(settings.repetition_penalty),
        }
    }
}

impl From<crate::application::MemoryReport> for Memory {
    fn from(memory: crate::application::MemoryReport) -> Self {
        Self {
            required_bytes: memory.estimate.required_bytes,
            available_bytes: memory.available,
            budget_bytes: memory.budget,
            source: memory.source,
            fit: memory.fit.as_str().to_owned(),
            max_safe_context_tokens: memory.max_safe_context,
            configured_cache_tokens: memory.estimate.cache_capacity_tokens,
        }
    }
}
