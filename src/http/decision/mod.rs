//! `POST /v1/decide`: Laya typed decisions for manual testing.

mod types;
mod worker;

use std::path::PathBuf;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use libmir::decision::{DecisionCheckpoint, DecisionModel, DecisionSchema};
use types::{BackendName, DecideRequest, DecideResponse, FieldAnswer};
use worker::Worker;

use crate::http::{ApiState, error::ApiError};

/// Requests admitted behind the running one; further requests get 503.
const QUEUE_CAPACITY: usize = 4;

/// The one resident checkpoint; a request for another unloads it first, so
/// the endpoint never holds more than one decision model.
type Loaded = Option<((PathBuf, BackendName), DecisionModel)>;

/// One worker thread owns the loaded checkpoint: Metal models hold
/// thread-bound MLX state, so they never cross threads.
#[derive(Clone)]
pub struct Decisions {
    worker: Worker<DecideRequest, Result<DecideResponse, ApiError>>,
}

impl Default for Decisions {
    fn default() -> Self {
        let worker = Worker::spawn(QUEUE_CAPACITY, || {
            let mut loaded: Loaded = None;
            move |request, cancelled: &dyn Fn() -> bool| answer(&mut loaded, request, cancelled)
        });
        Self { worker }
    }
}

pub async fn decide(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<DecideRequest>, JsonRejection>,
) -> Result<Json<DecideResponse>, ApiError> {
    state.authorize(&headers)?;
    let request = payload.map_err(|error| ApiError::bad_request(error.body_text()))?.0;
    state
        .decisions()
        .worker
        .submit(request)?
        .await
        .map_err(|_| ApiError::internal("the decision worker stopped"))?
        .map(Json)
}

fn answer(
    loaded: &mut Loaded,
    request: DecideRequest,
    cancelled: &dyn Fn() -> bool,
) -> Result<DecideResponse, ApiError> {
    let schema = DecisionSchema::from_json_schema(&request.schema)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let decision_state = request.state.into_state()?;
    let key = (request.checkpoint, request.backend);
    let model = match loaded.take() {
        Some((current, model)) if current == key => &loaded.insert((current, model)).1,
        previous => {
            // Refuse an unreadable checkpoint before giving up the resident one.
            if let Err(error) = DecisionCheckpoint::inspect(&key.0) {
                *loaded = previous;
                return Err(ApiError::bad_request(error.to_string()));
            }
            drop(previous);
            let model = DecisionModel::load(&key.0, key.1.backend())
                .map_err(|error| ApiError::bad_request(error.to_string()))?;
            &loaded.insert((key, model)).1
        },
    };
    if cancelled() {
        return Err(ApiError::internal("the decision request was cancelled"));
    }
    let answers = model
        .decide(&decision_state, &schema.questions())
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let values = schema
        .project(&answers)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let fields = schema
        .fields()
        .iter()
        .zip(answers)
        .map(|(field, answer)| FieldAnswer::new(field, answer))
        .collect();
    Ok(DecideResponse { values, fields })
}

#[cfg(test)]
mod tests;
