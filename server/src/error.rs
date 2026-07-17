use std::error::Error as StdError;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("configuration error: {0}")]
    Config(#[from] config::Error),
    #[error("startup error: {0}")]
    Startup(String),
    #[error("startup io error: {0}")]
    StartupIo(#[from] std::io::Error),
    #[error("startup tracing error: {0}")]
    StartupTracing(#[from] Box<dyn StdError + Send + Sync>),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("inference error: {0}")]
    Inference(#[from] libmir::Error),
    #[error("inference task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorMessage,
}

#[derive(Debug, Serialize)]
struct ErrorMessage {
    message: String,
    kind: String,
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Config(_)
            | Self::Startup(_)
            | Self::StartupIo(_)
            | Self::StartupTracing(_)
            | Self::Inference(_)
            | Self::Join(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = ErrorBody {
            error: ErrorMessage {
                message: self.to_string(),
                kind: status.as_str().to_owned(),
            },
        };
        (status, Json(body)).into_response()
    }
}
