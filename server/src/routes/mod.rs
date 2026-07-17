mod health;
mod v1;

use axum::{Router, routing::get};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health::health)).nest("/v1", v1::router())
}
