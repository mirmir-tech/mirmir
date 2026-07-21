use std::cmp::Ordering;

use sysinfo::System;

use super::hub::HubModel;

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct MachineMemory {
    pub total: Option<u64>,
    pub available: Option<u64>,
    pub budget: Option<u64>,
    pub source: &'static str,
}

#[derive(Debug)]
pub struct CatalogModel {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub gated: bool,
    pub model_class: String,
    pub compatibility: &'static str,
    pub memory_fit: &'static str,
    pub weight_bytes: Option<u64>,
    pub required_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub confidence: &'static str,
    pub reason: String,
    pub downloaded: bool,
    pub local_source: &'static str,
    pub library: String,
    pub features: Features,
}

#[derive(Debug, Default)]
pub struct Features {
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

impl MachineMemory {
    #[must_use]
    pub fn detect() -> Self {
        if !cfg!(target_os = "macos") {
            return Self {
                total: None,
                available: None,
                budget: None,
                source: "device telemetry unavailable",
            };
        }
        let mut system = System::new();
        system.refresh_memory();
        let total = system.total_memory();
        let available = system.available_memory();
        let reserve = (total / 10).max(GIB);
        Self {
            total: Some(total),
            available: Some(available),
            budget: Some(available.saturating_sub(reserve)),
            source: "macOS unified system memory",
        }
    }

    #[must_use]
    pub fn from_runtime(memory: &libmir::MemorySnapshot) -> Self {
        let reserve = memory.total_bytes.map_or(GIB, |total| (total / 10).max(GIB));
        Self {
            total: memory.total_bytes,
            available: memory.available_bytes,
            budget: memory.available_bytes.map(|available| available.saturating_sub(reserve)),
            source: if memory.unified {
                "unified accelerator memory"
            } else {
                "accelerator memory"
            },
        }
    }
}

pub fn evaluate(model: HubModel, memory: MachineMemory) -> CatalogModel {
    let model_class = model.model_class();
    let compatibility = compatibility(&model);
    let weight_bytes = weights(&model);
    let required_bytes = weight_bytes.map(required);
    let memory_fit = fit(required_bytes, memory.budget);
    let confidence = if required_bytes.is_some() && memory.budget.is_some() {
        "medium"
    } else {
        "low"
    };
    let reason = reason(compatibility, memory_fit);
    let gated = model.is_gated();
    let library = library(&model);
    let identity = format!("{} {model_class}", model.id).to_ascii_lowercase();
    let generation = !identity.contains("embedding") && !identity.contains("rerank");
    let qwen3 = identity.contains("qwen3");
    let tool_use = feature(&model, &["tool-use", "tool-calling", "function-calling"])
        || generation && (qwen3 || identity.contains("qwen2.5") && identity.contains("instruct"));
    let thinking = feature(&model, &["reasoning", "thinking"])
        || generation && (qwen3 || identity.contains("qwq") || identity.contains("deepseek-r1"));
    let vision = feature(&model, &["image-text-to-text", "multimodal", "vision"])
        || model_class.to_ascii_lowercase().contains("vision");
    CatalogModel {
        id: model.id,
        downloads: model.downloads,
        likes: model.likes,
        gated,
        model_class,
        compatibility,
        memory_fit,
        weight_bytes,
        required_bytes,
        budget_bytes: memory.budget,
        confidence,
        reason,
        downloaded: false,
        local_source: "remote",
        library,
        features: Features { tool_use, thinking, vision },
    }
}

fn library(model: &HubModel) -> String {
    if model.tags.iter().any(|tag| tag.eq_ignore_ascii_case("gguf")) {
        return "GGUF".to_owned();
    }
    match model.library_name.as_deref() {
        Some(value) if value.eq_ignore_ascii_case("mlx") => "MLX".to_owned(),
        Some(value) if value.eq_ignore_ascii_case("transformers") => "Transformers".to_owned(),
        Some(value) => value.to_owned(),
        None if model.tags.iter().any(|tag| tag.eq_ignore_ascii_case("safetensors")) => {
            "SafeTensors".to_owned()
        },
        None => "Unknown".to_owned(),
    }
}

fn feature(model: &HubModel, tags: &[&str]) -> bool {
    model
        .tags
        .iter()
        .any(|tag| tags.iter().any(|candidate| tag.eq_ignore_ascii_case(candidate)))
}

pub fn compare(left: &CatalogModel, right: &CatalogModel) -> Ordering {
    rank(left)
        .cmp(&rank(right))
        .then_with(|| right.downloads.cmp(&left.downloads))
        .then_with(|| left.id.cmp(&right.id))
}

fn rank(model: &CatalogModel) -> u8 {
    match (model.compatibility, model.memory_fit) {
        ("supported", "fits") => 0,
        ("supported", "tight") => 1,
        ("supported", "unknown") => 2,
        ("supported", "does_not_fit") => 3,
        ("unknown", "fits") => 4,
        ("unknown", "tight") => 5,
        ("unknown", "unknown") => 6,
        ("unknown", "does_not_fit") => 7,
        _ => 8,
    }
}

fn compatibility(model: &HubModel) -> &'static str {
    if model.safetensors.is_none() || model.tags.iter().any(|tag| tag == "gguf") {
        return "unsupported";
    }
    "unknown"
}

fn weights(model: &HubModel) -> Option<u64> {
    let parameters = &model.safetensors.as_ref()?.parameters;
    (!parameters.is_empty()).then(|| {
        parameters.iter().try_fold(0_u64, |total, (dtype, count)| {
            let bits = dtype_bits(dtype)?;
            Some(total.saturating_add(count.saturating_mul(bits).div_ceil(8)))
        })
    })?
}

fn dtype_bits(dtype: &str) -> Option<u64> {
    match dtype.to_ascii_uppercase().as_str() {
        "F64" => Some(64),
        "F32" => Some(32),
        "BF16" | "F16" => Some(16),
        "F8" | "FP8" | "I8" | "U8" => Some(8),
        "I4" | "U4" | "FP4" | "NVFP4" => Some(4),
        _ => None,
    }
}

fn required(weights: u64) -> u64 {
    let workspace = (weights / 5).max(GIB / 2);
    weights.saturating_add(workspace).saturating_add(GIB)
}

const fn fit(required: Option<u64>, budget: Option<u64>) -> &'static str {
    match (required, budget) {
        (Some(required), Some(budget)) if required <= budget.saturating_mul(85) / 100 => "fits",
        (Some(required), Some(budget)) if required <= budget => "tight",
        (Some(_), Some(_)) => "does_not_fit",
        _ => "unknown",
    }
}

fn reason(compatibility: &str, memory_fit: &str) -> String {
    match (compatibility, memory_fit) {
        ("unsupported", _) => {
            "model contract or weight format is not supported by libmir".to_owned()
        },
        ("unknown", "fits") => {
            "estimated weights fit; download is required to inspect the execution contract"
                .to_owned()
        },
        ("unknown", "tight") => {
            "estimated usage is tight; download is required to inspect the execution contract"
                .to_owned()
        },
        ("unknown", "does_not_fit") => {
            "estimated usage exceeds the current memory budget".to_owned()
        },
        ("unknown", "unknown") => {
            "download is required to inspect the model configuration and tensors".to_owned()
        },
        (_, "fits") => "estimated weights and runtime reserve fit the current budget".to_owned(),
        (_, "tight") => "estimated usage leaves less than 15% headroom".to_owned(),
        (_, "does_not_fit") => "estimated usage exceeds the current memory budget".to_owned(),
        _ => "insufficient Hub or device metadata for a reliable estimate".to_owned(),
    }
}

#[cfg(test)]
mod tests;
