use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("configuration error: {0}")]
    Config(#[from] config::Error),
    #[error("cli error: {0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("integer conversion failed: {0}")]
    Integer(#[from] std::num::TryFromIntError),
    #[error("integer parse failed: {0}")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("model error: {0}")]
    Model(#[from] libmir::models::ModelsError),
    #[error("inference error: {0}")]
    Inference(#[from] libmir::Error),
    #[error("runtime error: {0}")]
    Runtime(#[from] libmir::runtime::RuntimeError),
}
