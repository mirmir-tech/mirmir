use std::{fmt, path::PathBuf};

use super::{Instance, SYSTEM_SOCKET};

/// Where a connection reaches its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    Socket(PathBuf),
    /// `host:port` of a remote TLS listener.
    Remote(String),
}

impl fmt::Display for Address {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Socket(socket) => write!(formatter, "{}", socket.display()),
            Self::Remote(address) => write!(formatter, "grpcs://{address}"),
        }
    }
}

/// A connected server identity, with the resolved address for personal
/// instances.
#[derive(Debug, Clone)]
pub enum Endpoint {
    Personal(PathBuf),
    System,
    Selected(PathBuf),
    Remote(String),
}

impl Endpoint {
    pub fn new(instance: &Instance, address: Address) -> Self {
        match (instance, address) {
            (Instance::System, _) => Self::System,
            (Instance::Remote(target), _) => Self::Remote(target.address.clone()),
            (_, Address::Remote(address)) => Self::Remote(address),
            (Instance::Personal, Address::Socket(socket)) => Self::Personal(socket),
            (Instance::Socket(_), Address::Socket(socket)) => Self::Selected(socket),
        }
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Personal(_) => "PERSONAL",
            Self::System => "SYSTEM",
            Self::Selected(_) => "ATTACHED",
            Self::Remote(_) => "REMOTE",
        }
    }

    pub fn address(&self) -> Address {
        match self {
            Self::Personal(socket) | Self::Selected(socket) => Address::Socket(socket.clone()),
            Self::System => Address::Socket(PathBuf::from(SYSTEM_SOCKET)),
            Self::Remote(address) => Address::Remote(address.clone()),
        }
    }
}
