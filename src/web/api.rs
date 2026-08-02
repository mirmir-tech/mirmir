use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tonic::Request;

use super::{
    security_headers,
    session::{TTL_SECONDS, WebError, validate_origin},
    types::{Models, Overview},
};
use crate::{
    http::ApiState,
    rpc::{proto, proto::runtime_server::Runtime},
};

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
    response
        .headers_mut()
        .insert(header::SET_COOKIE, super::result::runtime(HeaderValue::from_str(&cookie))?);
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
    let snapshot = super::result::runtime(
        state.service().telemetry(Request::new(proto::TelemetryRequest {})).await,
    )?
    .into_inner();
    Ok((security_headers(), Json(Overview::from(snapshot))).into_response())
}

pub async fn telemetry_history(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<TelemetryHistoryQuery>,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let history = super::result::status(
        state
            .service()
            .telemetry_history(Request::new(proto::TelemetryHistoryRequest {
                limit: query.limit.unwrap_or(900),
            }))
            .await,
    )?
    .into_inner();
    Ok((security_headers(), Json(history)).into_response())
}

pub async fn models(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let models = super::result::runtime(
        state
            .service()
            .list_local_models(Request::new(proto::ListLocalModelsRequest {}))
            .await,
    )?
    .into_inner()
    .models
    .into_iter()
    .map(Into::into)
    .collect();
    Ok((security_headers(), Json(Models { models })).into_response())
}
