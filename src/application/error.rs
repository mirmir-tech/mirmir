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
    #[error("insufficient accelerator memory: {0}")]
    MemoryPressure(String),
    #[error("model persistence failed: {0}")]
    Persistence(String),
    #[error(transparent)]
    Configuration(#[from] crate::error::Error),
    #[error("inference failed: {0}")]
    Inference(#[from] libmir::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
