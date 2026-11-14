use std::net::SocketAddr;

use super::super::{Client, KeyRecord, listener::Listener, tls::CertificateSource};
use crate::remote::Fingerprint;

#[derive(Debug, Clone)]
pub struct KeyStatus {
    pub record: KeyRecord,
    pub last_used_unix_ms: Option<u64>,
}

/// A running listener's address and certificate.
#[derive(Debug, Clone)]
pub struct Listening {
    pub address: SocketAddr,
    pub fingerprint: Fingerprint,
    pub source: CertificateSource,
}

impl From<&Listener> for Listening {
    fn from(listener: &Listener) -> Self {
        Self {
            address: listener.address,
            fingerprint: listener.identity.fingerprint,
            source: listener.identity.source,
        }
    }
}

/// Whether remote access is on and, when it is, what its listener does.
#[derive(Debug, Clone)]
pub enum Mode {
    /// `server.remote_bind` is unset.
    Off,
    On {
        bind: String,
        listener: ListenerState,
    },
}

#[derive(Debug, Clone)]
pub enum ListenerState {
    Listening(Listening),
    /// The configured listener could not start or be restored.
    Failed(String),
    /// Not running: the server was started on demand by a command, which
    /// never opens the listener, or it is shutting down.
    Stopped,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub mode: Mode,
    pub keys: Vec<KeyStatus>,
    pub clients: Vec<Client>,
}

#[cfg(test)]
impl Status {
    /// The configured listen address; `None` when remote access is off.
    pub fn bind(&self) -> Option<&str> {
        match &self.mode {
            Mode::Off => None,
            Mode::On { bind, .. } => Some(bind),
        }
    }

    pub const fn listening(&self) -> Option<&Listening> {
        match &self.mode {
            Mode::On {
                listener: ListenerState::Listening(listening),
                ..
            } => Some(listening),
            _ => None,
        }
    }
}

/// The live listener of one server. Each variant holds exactly what that
/// state needs, so a running listener never carries an error.
#[derive(Default)]
pub(super) enum Slot {
    /// Nothing runs; the configuration decides between off and stopped.
    #[default]
    Idle,
    Running {
        bind: String,
        listener: Listener,
    },
    Failed {
        bind: String,
        error: String,
    },
    /// The server is shutting down: no listener starts again.
    Closed,
}

impl Slot {
    /// The mode this slot presents when `configured` is the persisted
    /// address. A running or failed listener reports the address it was
    /// started for; remote access only changes that address together with
    /// the listener, so the two agree.
    pub(super) fn mode(&self, configured: Option<String>) -> Mode {
        let (bind, listener) = match (self, configured) {
            (Self::Running { bind, listener }, _) => {
                (bind.clone(), ListenerState::Listening(listener.into()))
            },
            (Self::Failed { bind, error }, _) => {
                (bind.clone(), ListenerState::Failed(error.clone()))
            },
            (Self::Idle | Self::Closed, Some(bind)) => (bind, ListenerState::Stopped),
            (Self::Idle | Self::Closed, None) => return Mode::Off,
        };
        Mode::On { bind, listener }
    }

    /// Takes a running listener out, leaving the slot idle unless closed.
    pub(super) fn take(&mut self) -> Option<Listener> {
        if matches!(self, Self::Closed) {
            return None;
        }
        match std::mem::take(self) {
            Self::Running { listener, .. } => Some(listener),
            Self::Idle | Self::Failed { .. } | Self::Closed => None,
        }
    }
}
