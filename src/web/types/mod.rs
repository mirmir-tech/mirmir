use serde::Serialize;

mod conversion;

#[derive(Serialize)]
pub struct Overview {
    pub sampled_at_unix_ms: u64,
    pub uptime_ms: u64,
    pub loaded_models: u64,
    pub active_requests: u64,
    pub total_requests: u64,
    pub completed_requests: u64,
    pub failed_requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub current_tokens_per_second: Option<f64>,
    pub current_prefill_tokens_per_second: Option<f64>,
    pub current_decode_tokens_per_second: Option<f64>,
    pub current_ttft_ms: Option<f64>,
    pub last_tokens_per_second: Option<f64>,
    pub mean_tokens_per_second: Option<f64>,
    pub last_prefill_tokens_per_second: Option<f64>,
    pub mean_prefill_tokens_per_second: Option<f64>,
    pub last_decode_tokens_per_second: Option<f64>,
    pub mean_decode_tokens_per_second: Option<f64>,
    pub last_ttft_ms: Option<f64>,
    pub mean_ttft_ms: Option<f64>,
    pub host_total_memory_bytes: Option<u64>,
    pub host_available_memory_bytes: Option<u64>,
    pub memory_source: String,
    pub kv_total_blocks: u64,
    pub kv_used_blocks: u64,
    pub kv_cached_prefixes: u64,
    pub kv_hit_tokens: u64,
    pub kv_miss_tokens: u64,
    pub active_elapsed_ms: f64,
    pub active_prompt_tokens: u64,
    pub active_completion_tokens: u64,
    pub active_stage: String,
    pub gpu_utilization_percent: Option<f64>,
    pub device_temperature_celsius: Option<f64>,
    pub device_power_watts: Option<f64>,
    pub device_power_limit_watts: Option<f64>,
    pub device_name: String,
}

#[derive(Serialize)]
pub struct Models {
    pub models: Vec<Model>,
}

#[derive(Serialize)]
pub struct Model {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub path: String,
    pub state: String,
    pub selector: String,
    pub managed: bool,
    pub image_input: bool,
    pub image_unavailable_reason: String,
    pub loadable: bool,
    pub load_unavailable_reason: String,
    #[serde(rename = "model_class")]
    pub class: String,
    pub library: String,
    pub ecosystem: String,
    pub container: String,
    pub encoding: String,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
    pub size_bytes: u64,
    #[serde(flatten)]
    pub features: Features,
}

#[derive(Serialize)]
pub struct CatalogResults {
    pub models: Vec<CatalogModel>,
    pub total_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub memory_source: String,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct CatalogModel {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub gated: bool,
    pub model_class: String,
    pub compatibility: String,
    pub memory_fit: String,
    pub estimated_required_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub confidence: String,
    pub reason: String,
    pub downloaded: bool,
    pub local_source: String,
    pub estimated_weight_bytes: Option<u64>,
    pub library: String,
    pub ecosystem: String,
    pub container: String,
    pub encoding: String,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
    pub preflight_bytes: Option<u64>,
    pub preflight_error: Option<String>,
    #[serde(flatten)]
    pub features: Features,
}

#[derive(Serialize)]
pub struct Features {
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

#[derive(Serialize)]
pub struct Activity {
    pub operation_id: String,
    pub kind: String,
    pub target: String,
    pub state: String,
    pub stage: String,
    pub detail: String,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
    pub current: Option<u64>,
    pub total: Option<u64>,
}
