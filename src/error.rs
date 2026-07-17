use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("another mirmir server owns {0}")]
    AlreadyRunning(PathBuf),
    #[error("model `{0}` is not configured and is not a local path")]
    ModelNotFound(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("dotenv error: {0}")]
    Dotenv(#[from] dotenvy::Error),
    #[error("TOML decode error: {0}")]
    TomlDecode(#[from] toml::de::Error),
    #[error("TOML encode error: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Hugging Face Hub error: {0}")]
    Hub(#[from] hf_hub::HFError),
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("gRPC request failed: {0}")]
    Status(#[from] tonic::Status),
    #[error("server task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("inference error: {0}")]
    Inference(#[from] libmir::Error),
    #[error("numeric conversion failed: {0}")]
    Integer(#[from] std::num::TryFromIntError),
}
