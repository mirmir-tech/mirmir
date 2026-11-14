use super::{KeyRecord, Scope};

/// Who made one `Runtime` call. The TLS listener's authentication layer
/// attaches a [`Caller`] to every call it admits; the local Unix socket has no
/// such layer, so its calls carry none and come from the server's operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Local,
    Remote(Caller),
}

/// The remote API key a call on the TLS listener presented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub key_id: String,
    pub key_name: String,
    pub scope: Scope,
}

impl From<&KeyRecord> for Caller {
    fn from(key: &KeyRecord) -> Self {
        Self {
            key_id: key.id.clone(),
            key_name: key.name.clone(),
            scope: key.scope,
        }
    }
}

impl Origin {
    #[must_use]
    pub fn of<T>(request: &tonic::Request<T>) -> Self {
        request.extensions().get::<Caller>().cloned().map_or(Self::Local, Self::Remote)
    }

    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}
