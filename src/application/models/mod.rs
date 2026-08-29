mod lifecycle;
mod local;
mod preflight;
mod residency;
pub mod state;

use libmir::{Model, ModelDescriptor};
pub use preflight::{Check, MemoryReport, eviction_can_help, rejection, safe_context};
use residency::ModelResidency;
pub use residency::eviction_candidate;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct LocalModelInfo {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub path: String,
    pub state: String,
    pub recent_rank: Option<u32>,
    pub selector: String,
    pub managed: bool,
    pub image_input: bool,
    pub image_unavailable_reason: String,
    pub model_class: String,
    pub loadable: bool,
    pub load_unavailable_reason: String,
    pub library: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub id: String,
    pub path: String,
    pub image_input: bool,
    pub image_unavailable_reason: String,
}

pub struct ModelEntry {
    pub model: Model,
    pub info: ModelInfo,
    pub last_used: u64,
}

impl ModelInfo {
    #[must_use]
    pub fn loaded(id: String, path: String, model: &Model) -> Self {
        let (image_input, image_unavailable_reason) = image_capability(model.descriptor());
        Self {
            id,
            path,
            image_input,
            image_unavailable_reason,
        }
    }
}

impl Clone for ModelEntry {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            info: self.info.clone(),
            last_used: self.last_used,
        }
    }
}

fn image_capability(descriptor: &ModelDescriptor) -> (bool, String) {
    if descriptor.vision().is_none() {
        return (
            false,
            "model configuration does not declare a supported image execution contract".to_owned(),
        );
    }
    match descriptor.vision_readiness() {
        Some(readiness) if !readiness.is_ready() => (
            false,
            format!("{} required vision tensors are missing", readiness.missing.len()),
        ),
        None => (false, "image execution contract readiness is unavailable".to_owned()),
        Some(_) if descriptor.image_processor().is_none() => {
            (false, "checkpoint does not provide a supported image processor".to_owned())
        },
        Some(_) => (true, String::new()),
    }
}
