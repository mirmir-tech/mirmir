use serde::{Deserialize, Serialize};

pub mod catalog;

#[derive(Clone, Default, Deserialize)]
pub struct Bootstrap {
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
    pub state: String,
    pub selector: String,
    pub image_input: bool,
    pub load_unavailable_reason: String,
    pub ecosystem: String,
    pub container: String,
    pub encoding: String,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
    pub size_bytes: u64,
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

#[derive(Clone, Default, Deserialize)]
pub struct Models {
    pub models: Vec<Model>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Activity {
    pub operation_id: String,
    pub kind: String,
    pub target: String,
    pub state: String,
    pub detail: String,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
    pub current: Option<u64>,
    pub total: Option<u64>,
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
