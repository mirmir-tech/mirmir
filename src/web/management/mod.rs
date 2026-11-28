use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
mod context;
mod types;

use self::types::{Accepted, Cancelled, Inspection, Memory, Removed, Settings, Unloaded};
pub use self::types::{CancelRequest, InspectQuery, LoadRequest, RemoveRequest, SelectorRequest};
use super::{capabilities::TaskCapabilities, security_headers, session::WebError};
use crate::{
    application::{self, LoadDefaults, ModelDefaults},
    config::HubModelConfig,
    http::ApiState,
};

pub async fn inspect(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<InspectQuery>,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let choice = context::query(query.context.as_deref())
        .map_err(|message| WebError::application(application::Error::InvalidContext(message)))?;
    let inspected = state
        .application()
        .inspect_model(&query.selector, choice)
        .map_err(WebError::application)?;
    let response = Inspection {
        settings: inspected.settings.map(Settings::from),
        has_mirmir_overrides: inspected.has_mirmir_overrides,
        memory: Memory::from(inspected.memory),
        task: inspected.task.to_owned(),
        capabilities: Some(TaskCapabilities::from(inspected.capabilities)),
        context: inspected.context.map(Into::into),
    };
    Ok((security_headers(), Json(response)).into_response())
}

pub async fn load(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<LoadRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let hub = (!request.repo_id.is_empty())
        .then_some(HubModelConfig::new(request.repo_id, request.revision, request.commit));
    let defaults = ModelDefaults {
        generation: request.settings.map(Into::into),
        context: request.context.map(Into::into),
    };
    let load = LoadDefaults {
        config_id: request.config_id,
        hub,
        defaults,
    };
    // Saving a first configuration waits for the memory gate a running
    // load holds; keep that wait off the async workers.
    let application = state.application().clone();
    let selector = request.selector;
    let prepared =
        tokio::task::spawn_blocking(move || application.prepare_load(&selector, load)).await;
    let selector = prepared.map_err(WebError::join)?.map_err(WebError::application)?;
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
    let application = state.application().clone();
    let unloaded = tokio::task::spawn_blocking(move || application.unload_model(&request.selector))
        .await
        .map_err(WebError::join)?
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
