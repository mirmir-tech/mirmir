use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tonic::Request;

use super::{security_headers, session::WebError};
use crate::{
    http::ApiState,
    rpc::{proto, proto::runtime_server::Runtime},
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
    settings: Settings,
    has_mirmir_overrides: bool,
    memory: Memory,
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
        .service()
        .inspect_model(Request::new(proto::InspectModelRequest { selector: query.selector }))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
    let settings = inspected.settings.ok_or_else(|| WebError::runtime("settings missing"))?;
    let memory = inspected.memory.ok_or_else(|| WebError::runtime("memory estimate missing"))?;
    let response = Inspection {
        settings: Settings::from(settings),
        has_mirmir_overrides: inspected.has_mirmir_overrides,
        memory: Memory::from(memory),
    };
    Ok((security_headers(), Json(response)).into_response())
}

pub async fn load(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<LoadRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let mut events = state
        .service()
        .load_model(Request::new(proto::LoadModelRequest {
            selector: request.selector,
            settings: request.settings.map(Into::into),
            force: request.force,
            ..Default::default()
        }))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
    drop(tokio::spawn(async move { while events.next().await.is_some() {} }));
    Ok(accepted("load"))
}

pub async fn unload(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<SelectorRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let unloaded = state
        .service()
        .unload_model(Request::new(proto::UnloadModelRequest { selector: request.selector }))
        .await
        .map_err(WebError::from_status)?
        .into_inner()
        .unloaded;
    Ok((security_headers(), Json(Unloaded { unloaded })).into_response())
}

pub async fn remove(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<RemoveRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let removed = state
        .service()
        .remove_model(Request::new(proto::RemoveModelRequest { repo_id: request.repo_id }))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
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
    let cancelled = state
        .service()
        .cancel_operation(Request::new(proto::CancelOperationRequest {
            operation_id: request.operation_id,
        }))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
    Ok((
        security_headers(),
        Json(Cancelled {
            found: cancelled.found,
            accepted: cancelled.accepted,
            state: cancelled.state,
        }),
    )
        .into_response())
}

fn accepted(operation: &'static str) -> Response {
    (security_headers(), Json(Accepted { accepted: true, operation })).into_response()
}

impl From<proto::GenerationSettings> for Settings {
    fn from(settings: proto::GenerationSettings) -> Self {
        Self {
            max_tokens: settings.max_tokens,
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            repetition_penalty: settings.repetition_penalty,
        }
    }
}

impl From<Settings> for proto::GenerationSettings {
    fn from(settings: Settings) -> Self {
        Self {
            max_tokens: settings.max_tokens,
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            repetition_penalty: settings.repetition_penalty,
        }
    }
}

impl From<proto::ModelMemoryEstimate> for Memory {
    fn from(memory: proto::ModelMemoryEstimate) -> Self {
        Self {
            required_bytes: memory.required_bytes,
            available_bytes: memory.available_bytes,
            budget_bytes: memory.budget_bytes,
            source: memory.memory_source,
            fit: memory.fit,
            max_safe_context_tokens: memory.max_safe_context_tokens,
            configured_cache_tokens: memory.configured_cache_tokens,
        }
    }
}
