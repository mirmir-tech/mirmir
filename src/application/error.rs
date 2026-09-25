#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("runtime is not ready for inference; wait for active model restoration")]
    RuntimeNotReady,
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
    #[error("configuration failed: {0}")]
    Configuration(String),
    #[error("external service failed: {0}")]
    External(String),
    #[error("application infrastructure failed: {0}")]
    Infrastructure(String),
    #[error("inference failed: {0}")]
    Inference(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    InvalidArgument,
    Conflict,
    ResourceExhausted,
    Cancelled,
    Unavailable,
    Internal,
}

impl Error {
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        match self {
            Self::InvalidModel(_) | Self::InvalidRequest(_) | Self::Configuration(_) => {
                ErrorClass::InvalidArgument
            },
            Self::ModelInUse(_) | Self::ModelAlreadyLoading(_) | Self::ModelLoaded(_) => {
                ErrorClass::Conflict
            },
            Self::MemoryPressure(_) => ErrorClass::ResourceExhausted,
            Self::Cancelled => ErrorClass::Cancelled,
            Self::External(_) | Self::RuntimeNotReady => ErrorClass::Unavailable,
            Self::StatePoisoned(_)
            | Self::Persistence(_)
            | Self::Infrastructure(_)
            | Self::Inference(_) => ErrorClass::Internal,
        }
    }
}

impl From<crate::error::Error> for Error {
    fn from(error: crate::error::Error) -> Self {
        match error {
            crate::error::Error::Cancelled => Self::Cancelled,
            crate::error::Error::Config(message) => Self::Configuration(message),
            error @ (crate::error::Error::Dotenv(_)
            | crate::error::Error::TomlDecode(_)
            | crate::error::Error::TomlEncode(_)) => Self::Configuration(error.to_string()),
            crate::error::Error::ModelNotFound(model) => Self::InvalidModel(model),
            crate::error::Error::Io(error) => Self::Persistence(error.to_string()),
            error @ (crate::error::Error::Http(_)
            | crate::error::Error::Hub(_)
            | crate::error::Error::Transport(_)
            | crate::error::Error::Status(_)) => Self::External(error.to_string()),
            crate::error::Error::Inference(error) => error.into(),
            error @ (crate::error::Error::AlreadyRunning(_)
            | crate::error::Error::Json(_)
            | crate::error::Error::Join(_)
            | crate::error::Error::Integer(_)) => Self::Infrastructure(error.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<libmir::Error> for Error {
    fn from(error: libmir::Error) -> Self {
        match error {
            libmir::Error::Cancelled => Self::Cancelled,
            error @ (libmir::Error::MemoryAdmission { .. }
            | libmir::Error::VisionResourceLimit { .. }) => Self::MemoryPressure(error.to_string()),
            error @ (libmir::Error::EmptyPrompt
            | libmir::Error::Runtime(libmir::RuntimeError::KvCapacity { .. })
            | libmir::Error::TaskMismatch { .. }
            | libmir::Error::Model(_)
            | libmir::Error::Context { .. }) => Self::InvalidRequest(error.to_string()),
            error => Self::Inference(error.to_string()),
        }
    }
}
