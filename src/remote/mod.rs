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
mod scope;
mod tls;

pub use access::{KeyStatus, RemoteAccess, Status};
pub use clients::Client;
pub use connect::{RemoteTarget, Trust, presented_fingerprint};
pub use keys::{KeyRecord, display_prefix};
pub use scope::{Access, Scope};
pub use tls::{CertificateSource, Fingerprint};

#[cfg(test)]
mod tests;
