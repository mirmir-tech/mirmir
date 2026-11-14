//! JSON shapes of the management API.

use serde::{Deserialize, Serialize};

use super::context::{ContextInfo, ContextJson};
use crate::{config::GenerationConfig, web::capabilities::TaskCapabilities};

#[derive(Deserialize)]
pub struct InspectQuery {
    pub(super) selector: String,
    /// `automatic`, or a token count: estimate memory for that context.
    #[serde(default)]
    pub(super) context: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Settings {
    pub(super) max_tokens: u64,
    pub(super) temperature: f32,
    pub(super) top_p: f32,
    pub(super) top_k: u64,
    pub(super) repetition_penalty: f32,
}
#[derive(Deserialize)]
pub struct LoadRequest {
    pub(super) selector: String,
    pub(super) settings: Option<Settings>,
    #[serde(default)]
    pub(super) config_id: String,
    #[serde(default)]
    pub(super) repo_id: String,
    #[serde(default)]
    pub(super) revision: String,
    #[serde(default)]
    pub(super) commit: String,
    #[serde(default)]
    pub(super) force: bool,
    /// Saved for the model, then used by this load.
    #[serde(default)]
    pub(super) context: Option<ContextJson>,
}
#[derive(Deserialize)]
pub struct SelectorRequest {
    pub(super) selector: String,
}
#[derive(Deserialize)]
pub struct RemoveRequest {
    pub(super) repo_id: String,
}
#[derive(Deserialize)]
pub struct CancelRequest {
    pub(super) operation_id: String,
}
#[derive(Serialize)]
pub(super) struct Inspection {
    pub(super) settings: Option<Settings>,
    pub(super) has_mirmir_overrides: bool,
    pub(super) memory: Memory,
    pub(super) task: String,
    pub(super) capabilities: Option<TaskCapabilities>,
    pub(super) context: Option<ContextInfo>,
}
#[derive(Serialize)]
pub(super) struct Memory {
    pub(super) weight_bytes: u64,
    pub(super) kv_cache_bytes: u64,
    pub(super) workspace_bytes: u64,
    pub(super) required_bytes: u64,
    pub(super) available_bytes: Option<u64>,
    pub(super) budget_bytes: Option<u64>,
    #[serde(rename = "memory_source")]
    pub(super) source: String,
    pub(super) fit: String,
    pub(super) max_safe_context_tokens: Option<u64>,
    pub(super) configured_cache_tokens: u64,
}
#[derive(Serialize)]
pub(super) struct Accepted {
    pub(super) accepted: bool,
    pub(super) operation: &'static str,
}
#[derive(Serialize)]
pub(super) struct Unloaded {
    pub(super) unloaded: bool,
}
#[derive(Serialize)]
pub(super) struct Removed {
    pub(super) removed: bool,
    pub(super) freed_bytes: u64,
}
#[derive(Serialize)]
pub(super) struct Cancelled {
    pub(super) found: bool,
    pub(super) accepted: bool,
    pub(super) state: String,
}

impl From<libmir::GenerationSettings> for Settings {
    fn from(settings: libmir::GenerationSettings) -> Self {
        Self {
            max_tokens: u64::try_from(settings.max_tokens).unwrap_or(u64::MAX),
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: u64::try_from(settings.top_k).unwrap_or(u64::MAX),
            repetition_penalty: settings.repetition_penalty,
        }
    }
}

impl From<Settings> for GenerationConfig {
    fn from(settings: Settings) -> Self {
        Self {
            max_tokens: usize::try_from(settings.max_tokens).ok(),
            temperature: Some(settings.temperature),
            top_p: Some(settings.top_p),
            top_k: usize::try_from(settings.top_k).ok(),
            repetition_penalty: Some(settings.repetition_penalty),
        }
    }
}

impl From<crate::application::MemoryReport> for Memory {
    fn from(memory: crate::application::MemoryReport) -> Self {
        Self {
            weight_bytes: memory.estimate.weight_bytes,
            kv_cache_bytes: memory.estimate.kv_cache_bytes,
            workspace_bytes: memory.estimate.workspace_bytes,
            required_bytes: memory.estimate.required_bytes,
            available_bytes: memory.available,
            budget_bytes: memory.budget,
            source: memory.source,
            fit: memory.fit.as_str().to_owned(),
            max_safe_context_tokens: memory.max_safe_context,
            configured_cache_tokens: memory.estimate.cache_capacity_tokens,
        }
    }
}
