pub mod error;
pub mod routes;
pub mod state;

use axum::Router;
use state::AppState;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::router())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
