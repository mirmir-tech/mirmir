#[cfg(target_os = "linux")]
mod cuda;
mod environment;
mod error;
mod http;
#[cfg(target_os = "macos")]
mod metal;
mod runtime;

#[cfg(target_os = "linux")]
pub use cuda::CudaArgs;
pub use environment::{EnvironmentLoad, load_environment};
pub use error::{Error, Result};
pub use http::HttpArgs;
#[cfg(target_os = "macos")]
pub use metal::MetalArgs;
pub use runtime::RuntimeArgs;
