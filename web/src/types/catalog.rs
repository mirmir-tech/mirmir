use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
pub struct CatalogResults {
    pub models: Vec<CatalogModel>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub memory_fit: String,
    pub reason: String,
    pub downloaded: bool,
    pub estimated_weight_bytes: Option<u64>,
    pub library: String,
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

#[derive(Clone, Default, Deserialize)]
pub struct Inspection {
    pub settings: Option<GenerationSettings>,
    pub memory: Memory,
    pub task: String,
}

#[derive(Clone, Default, Deserialize, serde::Serialize)]
pub struct GenerationSettings {
    pub max_tokens: u64,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u64,
    pub repetition_penalty: f32,
}

#[derive(Clone, Default, Deserialize)]
pub struct Memory {
    pub required_bytes: u64,
    pub memory_source: String,
    pub fit: String,
    pub max_safe_context_tokens: Option<u64>,
    pub configured_cache_tokens: u64,
}

#[derive(Clone, Default, Deserialize)]
pub struct Removed {
    pub removed: bool,
    pub freed_bytes: u64,
}

#[derive(Clone, Default, Deserialize)]
pub struct UpdatedConfiguration {
    pub configuration: super::Configuration,
    pub message: String,
    pub restart_required: bool,
}
