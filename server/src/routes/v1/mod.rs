mod chat;
mod ws;

use axum::{
    Router,
    routing::{get, post},
};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chat/completions", post(chat::completion))
        .route("/ws", get(ws::upgrade))
}
