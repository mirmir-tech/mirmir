use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;

use super::{
    ApiState,
    error::ApiError,
    stream,
    types::{ChatRequest, CompletionResponse, HealthResponse, Model, ModelsResponse},
};
use crate::rpc::proto;

static NEXT_COMPLETION: AtomicU64 = AtomicU64::new(0);

impl ApiState {
    pub(super) fn authorize(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        let Some(expected) = &self.api_key else {
            return Ok(());
        };
        let supplied = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if supplied.is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes())) {
            Ok(())
        } else {
            Err(ApiError::unauthorized())
        }
    }
}

pub async fn health() -> Result<Json<HealthResponse>, ApiError> {
    let response = crate::application::RuntimeCoordinator::health();
    Ok(Json(HealthResponse {
        status: "ok",
        server_version: response.server_version.to_owned(),
        protocol_version: response.protocol_version.to_owned(),
    }))
}

pub async fn models(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<ModelsResponse>, ApiError> {
    state.authorize(&headers)?;
    let models = state
        .coordinator()
        .models()
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(ModelsResponse {
        object: "list",
        data: models
            .into_iter()
            .map(|model| Model {
                id: model.id,
                object: "model",
                created: 0,
                owned_by: "mirmir",
            })
            .collect(),
    }))
}

pub async fn chat(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<ChatRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    state.authorize(&headers)?;
    let request = match payload {
        Ok(request) => request.0,
        Err(error) => return Err(ApiError::bad_request(error.body_text())),
    };
    let streaming = request.stream;
    let include_usage =
        request.stream_options.as_ref().is_some_and(|options| options.include_usage);
    let model = request.model.clone();
    let id = completion_id();
    let created = unix_seconds();
    let mut events = state.service.generate_stream(request.into_proto()?);
    if streaming {
        return Ok(stream::response(events, id, created, model, include_usage, state.shutdown()));
    }
    while let Some(event) = events.next().await {
        let event = super::error::status(event)?;
        if let Some(proto::generate_event::Event::Completion(completion)) = event.event {
            return Ok(
                Json(CompletionResponse::new(id, created, model, completion)).into_response()
            );
        }
    }
    Err(ApiError::from_status(tonic::Status::internal(
        "generation ended without completion",
    )))
}

pub async fn not_found() -> ApiError {
    ApiError::not_found("route not found")
}

fn completion_id() -> String {
    let sequence = NEXT_COMPLETION.fetch_add(1, Ordering::Relaxed);
    format!("chatcmpl-mirmir-{}-{sequence}", unix_seconds())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let left = left.get(index).copied().unwrap_or(0);
        let right = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left ^ right);
    }
    difference == 0
}
