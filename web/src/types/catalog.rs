use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Deserialize)]
pub struct CatalogResults {
    pub models: Vec<CatalogModel>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    #[serde(default)]
    pub compatibility: String,
    pub memory_fit: String,
    pub reason: String,
    pub estimated_weight_bytes: Option<u64>,
    pub ecosystem: String,
    pub container: String,
    pub encoding: Option<String>,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
    /// A GGUF repository's quantizations, fewest bits first.
    #[serde(default)]
    pub variants: Vec<CatalogVariant>,
    pub default_variant: Option<String>,
}

/// One quantization of a GGUF repository; it downloads as `id:name`.
/// Whether it, or a repository, is downloaded the server's live model list
/// tells, as a pull or removal changes it, not the search.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
pub struct CatalogVariant {
    pub name: String,
    /// `exact`, `estimated` or `unknown`.
    pub size_kind: String,
    pub weight_bytes: Option<u64>,
    pub memory_fit: String,
}

/// A repository's variants with the sizes of their listed files.
#[derive(Clone, Default, Deserialize)]
pub struct ModelVariants {
    pub variants: Vec<CatalogVariant>,
    pub default_variant: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Inspection {
    pub settings: Option<GenerationSettings>,
    pub memory: Memory,
    pub task: String,
    pub capabilities: Option<Capabilities>,
    /// Absent for models without a K/V cache and from older servers.
    pub context: Option<ContextInfo>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct GenerationSettings {
    pub max_tokens: u64,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u64,
    pub repetition_penalty: f32,
}

#[derive(Clone, Default, Deserialize)]
pub struct Memory {
    #[serde(default)]
    pub weight_bytes: u64,
    #[serde(default)]
    pub kv_cache_bytes: u64,
    #[serde(default)]
    pub workspace_bytes: u64,
    pub required_bytes: u64,
    pub available_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub memory_source: String,
    pub fit: Fit,
    pub max_safe_context_tokens: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Fits,
    #[serde(rename = "does_not_fit")]
    Exceeds,
    #[default]
    #[serde(other)]
    Unknown,
}

impl Fit {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fits => "Fits",
            Self::Exceeds => "Does not fit",
            Self::Unknown => "Fit unknown",
        }
    }

    pub const fn class(self) -> &'static str {
        match self {
            Self::Fits => "fits",
            Self::Exceeds => "does-not-fit",
            Self::Unknown => "fit-unknown",
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Capabilities {
    pub max_input_tokens: u64,
}

#[derive(Clone, Default, Deserialize)]
pub struct ContextInfo {
    pub block_tokens: u64,
    pub min_tokens: u64,
    pub max_tokens: u64,
    pub saved: ContextChoice,
    pub automatic_tokens: u64,
    pub resident_tokens: Option<u64>,
}

/// `"automatic"`, or `{"tokens": N}`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextChoice {
    #[default]
    Automatic,
    Tokens(u64),
}

impl ContextChoice {
    /// The `context` query parameter of `/models/inspect`.
    pub fn query(self) -> String {
        match self {
            Self::Automatic => "automatic".to_owned(),
            Self::Tokens(tokens) => tokens.to_string(),
        }
    }
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
