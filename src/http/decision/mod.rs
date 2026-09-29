//! `POST /v1/decide`: Laya typed decisions for manual testing.

mod types;

use std::{
    collections::{HashMap, hash_map::Entry},
    path::PathBuf,
    sync::mpsc::{Sender, channel},
    thread,
};

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use libmir::decision::{DecisionModel, DecisionSchema};
use tokio::sync::oneshot;
use types::{BackendName, DecideRequest, DecideResponse, FieldAnswer};

use crate::http::{ApiState, error::ApiError};

type Reply = oneshot::Sender<Result<DecideResponse, ApiError>>;

/// One worker thread owns every loaded checkpoint: Metal models hold
/// thread-bound MLX state, so they never cross threads.
#[derive(Clone)]
pub struct Decisions {
    jobs: Sender<(DecideRequest, Reply)>,
}

impl Default for Decisions {
    fn default() -> Self {
        let (jobs, requests) = channel::<(DecideRequest, Reply)>();
        let _worker = thread::spawn(move || {
            let mut models = HashMap::new();
            for (request, reply) in requests {
                let _receiver_gone = reply.send(answer(&mut models, request));
            }
        });
        Self { jobs }
    }
}

pub async fn decide(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<DecideRequest>, JsonRejection>,
) -> Result<Json<DecideResponse>, ApiError> {
    state.authorize(&headers)?;
    let request = payload.map_err(|error| ApiError::bad_request(error.body_text()))?.0;
    let (reply, response) = oneshot::channel();
    state
        .decisions()
        .jobs
        .send((request, reply))
        .map_err(|_| ApiError::internal("the decision worker stopped"))?;
    response
        .await
        .map_err(|_| ApiError::internal("the decision worker stopped"))?
        .map(Json)
}

fn answer(
    models: &mut HashMap<(PathBuf, BackendName), DecisionModel>,
    request: DecideRequest,
) -> Result<DecideResponse, ApiError> {
    let schema = DecisionSchema::from_json_schema(&request.schema)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let decision_state = request.state.into_state()?;
    let model = match models.entry((request.checkpoint, request.backend)) {
        Entry::Occupied(loaded) => loaded.into_mut(),
        Entry::Vacant(slot) => {
            let (checkpoint, backend) = slot.key();
            let model = DecisionModel::load(checkpoint, backend.backend())
                .map_err(|error| ApiError::bad_request(error.to_string()))?;
            slot.insert(model)
        },
    };
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
