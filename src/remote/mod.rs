//! Remote access: the `Runtime` gRPC service on a second, TLS listener that
//! requires a scoped API key in every call, and the client side that pins the
//! server certificate. The local Unix socket is unchanged and needs no key.

mod access;
mod auth;
mod clients;
pub mod command;
mod connect;
mod keys;
mod listener;
mod origin;
mod scope;
mod tls;

pub use access::{KeyStatus, RemoteAccess, Status};
pub use clients::Client;
#[cfg(test)]
pub use connect::presented_fingerprint_within;
pub use connect::{CONNECT_TIMEOUT, RemoteTarget, Trust, presented_fingerprint, within};
pub use keys::{KeyRecord, display_prefix};
pub use origin::{Caller, Origin};
pub use scope::{Access, Scope, reserved};
pub use tls::{CertificateSource, Fingerprint};

#[cfg(test)]
mod tests;
