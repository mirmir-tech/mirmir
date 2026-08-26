mod client;
mod service;

pub use client::{Client, connect};
pub use service::RuntimeService;

pub use crate::application::PROTOCOL_VERSION;

#[allow(clippy::all, clippy::nursery, clippy::pedantic)]
pub mod proto {
    tonic::include_proto!("mirmir.v1");
}

pub const TELEMETRY_SAMPLING_INTERVAL_MS: u64 = service::SAMPLING_INTERVAL_MS;
