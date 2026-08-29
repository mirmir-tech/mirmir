#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is poisoned")]
    StatePoisoned(&'static str),
    #[error("model `{0}` is currently serving a request")]
    ModelInUse(String),
    #[error("model `{0}` is already loading")]
    ModelAlreadyLoading(String),
    #[error("model `{0}` must be unloaded before removal")]
    ModelLoaded(String),
    #[error("model request is invalid: {0}")]
    InvalidModel(String),
    #[error("generation request is invalid: {0}")]
    InvalidRequest(String),
    #[error("insufficient accelerator memory: {0}")]
    MemoryPressure(String),
    #[error("model persistence failed: {0}")]
    Persistence(String),
    #[error("operation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Configuration(#[from] crate::error::Error),
    #[error("inference failed: {0}")]
    Inference(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<libmir::Error> for Error {
    fn from(error: libmir::Error) -> Self {
        match error {
            libmir::Error::Cancelled => Self::Cancelled,
            error @ (libmir::Error::MemoryAdmission { .. }
            | libmir::Error::VisionResourceLimit { .. }) => Self::MemoryPressure(error.to_string()),
            error @ (libmir::Error::EmptyPrompt
            | libmir::Error::TaskMismatch { .. }
            | libmir::Error::Model(_)
            | libmir::Error::Context { .. }) => Self::InvalidRequest(error.to_string()),
            error => Self::Inference(error.to_string()),
        }
    }
}
