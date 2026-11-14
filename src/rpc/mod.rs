mod client;
mod service;

#[cfg(test)]
pub use client::connect_remote_within;
pub use client::{Client, connect, connect_remote, unavailable};
pub use service::RuntimeService;

pub use crate::application::PROTOCOL_VERSION;

#[allow(clippy::all, clippy::nursery, clippy::pedantic)]
pub mod proto {
    tonic::include_proto!("mirmir.v1");
}

/// The largest request either `Runtime` listener decodes: a maximum-size
/// image (`media::MAX_IMAGE_BYTES`) plus room for the conversation around it.
pub const MAX_REQUEST_BYTES: usize = crate::media::MAX_IMAGE_BYTES + 12 * 1024 * 1024;

/// The `Runtime` service as both the local socket and the remote listener
/// serve it, with the shared request-size bound.
#[must_use]
pub fn server(service: RuntimeService) -> proto::runtime_server::RuntimeServer<RuntimeService> {
    proto::runtime_server::RuntimeServer::new(service).max_decoding_message_size(MAX_REQUEST_BYTES)
}

pub const TELEMETRY_SAMPLING_INTERVAL_MS: u64 = service::SAMPLING_INTERVAL_MS;
