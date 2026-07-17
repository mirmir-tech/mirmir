use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct Health {
    status: &'static str,
    uptime_ms: u128,
}

pub async fn health(State(state): State<AppState>) -> Json<Health> {
    let uptime_ms = state
        .started_at()
        .elapsed()
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();

    Json(Health { status: "ok", uptime_ms })
}
