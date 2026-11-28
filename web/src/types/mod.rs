use serde::{Deserialize, Serialize};

mod activity;
pub mod catalog;

pub use activity::{Activity, ActivityKind, ActivityState, Compatibility};

#[derive(Clone, Default, Deserialize)]
pub struct Bootstrap {
    #[serde(default)]
    pub backend_support: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub architecture: String,
    pub schema_version: u32,
    pub server_version: String,
    pub protocol_version: String,
}

#[derive(Clone, Default, Deserialize)]
pub struct Overview {
    pub sampled_at_unix_ms: u64,
    pub uptime_ms: u64,
    pub loaded_models: u64,
    pub active_requests: u64,
    pub total_requests: u64,
    pub completed_requests: u64,
    pub current_prefill_tokens_per_second: Option<f64>,
    pub current_decode_tokens_per_second: Option<f64>,
    pub current_ttft_ms: Option<f64>,
    pub last_prefill_tokens_per_second: Option<f64>,
    pub last_decode_tokens_per_second: Option<f64>,
    pub last_ttft_ms: Option<f64>,
    pub host_total_memory_bytes: Option<u64>,
    pub host_available_memory_bytes: Option<u64>,
    pub memory_source: String,
    pub mean_prefill_tokens_per_second: Option<f64>,
    pub mean_decode_tokens_per_second: Option<f64>,
    pub mean_ttft_ms: Option<f64>,
    pub active_stage: String,
    pub gpu_utilization_percent: Option<f64>,
    pub device_temperature_celsius: Option<f64>,
    pub device_power_watts: Option<f64>,
    pub device_power_limit_watts: Option<f64>,
    pub device_name: String,
}

#[derive(Clone, Default, Deserialize)]
pub struct Model {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub state: ModelState,
    pub selector: String,
    pub image_input: bool,
    pub load_unavailable_reason: String,
    pub ecosystem: String,
    pub container: String,
    pub encoding: Option<String>,
    #[serde(default)]
    pub metal_compatibility: Compatibility,
    #[serde(default)]
    pub cuda_compatibility: Compatibility,
    pub size_bytes: u64,
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

/// A local model's state as the server spells it, parsed once here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Ready,
    Loading,
    Error,
    Available,
    Missing,
    Paused,
    /// The model a demonstration shows in use.
    Active,
    /// A pull this dashboard started, before the server lists it.
    Downloading,
    /// A state this dashboard does not know.
    #[default]
    #[serde(other)]
    Unknown,
}

impl ModelState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Loading => "loading",
            Self::Error => "error",
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Paused => "paused",
            Self::Active => "active",
            Self::Downloading => "downloading",
            Self::Unknown => "unknown",
        }
    }

    /// Whether the model's files are on the server whole, as the server
    /// counts a model downloaded: not paused, missing or still downloading.
    /// An `Error` is a model on disk that does not load.
    #[must_use]
    pub const fn installed(self) -> bool {
        matches!(self, Self::Ready | Self::Loading | Self::Available | Self::Active | Self::Error)
    }

    /// Whether the model is loaded.
    #[must_use]
    pub const fn loaded(self) -> bool {
        matches!(self, Self::Ready | Self::Active)
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Models {
    pub models: Vec<Model>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Startup {
    pub phase: String,
    pub detail: String,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Startup { startup: Startup },
    Overview { overview: Box<Overview> },
    Models { models: Models },
    Configuration { configuration: Configuration },
    Activity { activity: Activity },
    Error { message: String },
}

#[derive(Clone, Default, Deserialize)]
pub struct Configuration {
    pub values: Vec<Setting>,
    pub config_path: String,
    pub secrets_path: String,
    pub raw_toml: String,
}

#[derive(Clone, Default, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub source: String,
    pub restart_required: bool,
    pub kind: String,
    pub actions: Vec<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct History {
    pub samples: Vec<HistorySample>,
}

#[derive(Clone, Default, Deserialize)]
pub struct HistorySample {
    pub prefill_tokens_per_second: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
    pub sampled_at_unix_ms: u64,
    pub memory_total_bytes: Option<u64>,
    pub memory_available_bytes: Option<u64>,
    pub gpu_utilization_percent: Option<f64>,
    pub device_temperature_celsius: Option<f64>,
    pub device_power_watts: Option<f64>,
    pub device_power_limit_watts: Option<f64>,
}

#[derive(Clone, Default)]
pub struct TelemetryPoint {
    pub prefill_tokens_per_second: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
    pub sampled_at_unix_ms: u64,
    pub memory_percent: Option<f64>,
    pub gpu_percent: Option<f64>,
    pub temperature_celsius: Option<f64>,
    pub power_watts: Option<f64>,
    pub power_limit_watts: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub reasoning_content: Option<String>,
}
