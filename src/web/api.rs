use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use super::{
    security_headers,
    session::{TTL_SECONDS, WebError, validate_origin},
    types::{Models, Overview},
};
use crate::http::ApiState;

#[derive(Serialize)]
struct SessionResponse {
    csrf_token: String,
    expires_at_unix_seconds: u64,
    expires_in_seconds: u64,
}

#[derive(Serialize)]
struct Deleted {
    deleted: bool,
}

#[derive(Deserialize)]
pub struct TelemetryHistoryQuery {
    limit: Option<u32>,
}

#[derive(Serialize)]
struct TelemetryHistory {
    samples: Vec<crate::application::HistorySample>,
    retention_limit: usize,
    sampling_interval_ms: u64,
}

pub async fn create_session(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    validate_origin(&headers)?;
    let issued = state.sessions().issue()?;
    let cookie = format!(
        "mirmir_session={}; HttpOnly; SameSite=Strict; Path=/api/mirmir/v1; Max-Age={TTL_SECONDS}",
        issued.token
    );
    let mut response = Json(SessionResponse {
        csrf_token: issued.csrf,
        expires_at_unix_seconds: issued.expires_at,
        expires_in_seconds: TTL_SECONDS,
    })
    .into_response();
    response.headers_mut().extend(security_headers());
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|error| WebError::runtime(error.to_string()))?,
    );
    Ok(response)
}

pub async fn delete_session(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    let authenticated = state.sessions().authorize_mutation(&headers)?;
    state.sessions().remove(&authenticated)?;
    let mut response = Json(Deleted { deleted: true }).into_response();
    response.headers_mut().extend(security_headers());
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "mirmir_session=; HttpOnly; SameSite=Strict; Path=/api/mirmir/v1; Max-Age=0",
        ),
    );
    Ok(response)
}

pub async fn overview(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let snapshot = state.application().telemetry_snapshot().map_err(WebError::application)?;
    Ok((security_headers(), Json(Overview::from(snapshot))).into_response())
}

pub async fn telemetry_history(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TelemetryHistoryQuery>,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let history = state
        .application()
        .telemetry_history(query.limit.unwrap_or(900))
        .map_err(WebError::application)?;
    Ok((
        security_headers(),
        Json(TelemetryHistory {
            samples: history,
            retention_limit: crate::application::TELEMETRY_RETENTION_LIMIT,
            sampling_interval_ms: crate::application::SAMPLING_INTERVAL_MS,
        }),
    )
        .into_response())
}

pub async fn models(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let models = state
        .application()
        .local_models()
        .map_err(WebError::application)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok((security_headers(), Json(Models { models })).into_response())
}
