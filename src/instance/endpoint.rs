use std::path::{Path, PathBuf};

use super::{Instance, SYSTEM_SOCKET};

/// A connected server identity, with the resolved address for personal
/// instances.
#[derive(Debug, Clone)]
pub enum Endpoint {
    Personal(PathBuf),
    System,
    Selected(PathBuf),
}

impl Endpoint {
    pub fn new(instance: &Instance, socket: PathBuf) -> Self {
        match instance {
            Instance::Personal => Self::Personal(socket),
            Instance::System => Self::System,
            Instance::Socket(_) => Self::Selected(socket),
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Personal(_) => "PERSONAL",
            Self::System => "SYSTEM",
            Self::Selected(_) => "ATTACHED",
        }
    }

    pub fn socket(&self) -> &Path {
        match self {
            Self::Personal(socket) | Self::Selected(socket) => socket,
            Self::System => Path::new(SYSTEM_SOCKET),
        }
    }
}
