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
    pub architecture: String,
    pub compatibility: &'static str,
    pub memory_fit: &'static str,
    pub weight_bytes: Option<u64>,
    pub required_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub confidence: &'static str,
    pub reason: String,
    pub downloaded: bool,
    pub local_source: &'static str,
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
    let architecture = model.architecture();
    let compatibility = compatibility(&model, &architecture);
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
    CatalogModel {
        id: model.id,
        downloads: model.downloads,
        likes: model.likes,
        gated,
        architecture,
        compatibility,
        memory_fit,
        weight_bytes,
        required_bytes,
        budget_bytes: memory.budget,
        confidence,
        reason,
        downloaded: false,
        local_source: "remote",
    }
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
        ("unknown", _) => 4,
        _ => 5,
    }
}

fn compatibility(model: &HubModel, architecture: &str) -> &'static str {
    if model.safetensors.is_none() || model.tags.iter().any(|tag| tag == "gguf") {
        return "unsupported";
    }
    let architecture = architecture.to_ascii_lowercase();
    if ["bielik", "deepseek", "gemma", "glm", "qwen", "mistral", "mixtral", "llama"]
        .iter()
        .any(|family| architecture.contains(family))
    {
        "supported"
    } else if architecture == "unknown" {
        "unknown"
    } else {
        "unsupported"
    }
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
        ("unsupported", _) => "architecture or weight format is not supported by libmir".to_owned(),
        (_, "fits") => "estimated weights and runtime reserve fit the current budget".to_owned(),
        (_, "tight") => "estimated usage leaves less than 15% headroom".to_owned(),
        (_, "does_not_fit") => "estimated usage exceeds the current memory budget".to_owned(),
        _ => "insufficient Hub or device metadata for a reliable estimate".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::catalog::hub::{HubConfig, SafeTensors};

    fn model(dtype: &str, parameters: u64) -> HubModel {
        HubModel {
            id: "Qwen/Test".to_owned(),
            downloads: 10,
            likes: 2,
            gated: serde_json::Value::Bool(false),
            config: HubConfig {
                architectures: vec!["Qwen2ForCausalLM".to_owned()],
                model_type: Some("qwen2".to_owned()),
            },
            safetensors: Some(SafeTensors {
                parameters: BTreeMap::from([(dtype.to_owned(), parameters)]),
            }),
            tags: vec!["safetensors".to_owned()],
        }
    }

    #[test]
    fn estimates_bf16_weights_and_fits_with_headroom() {
        let memory = MachineMemory {
            total: Some(16 * GIB),
            available: Some(12 * GIB),
            budget: Some(10 * GIB),
            source: "test",
        };
        let result = evaluate(model("BF16", 1_000_000_000), memory);
        assert_eq!(result.compatibility, "supported");
        assert_eq!(result.weight_bytes, Some(2_000_000_000));
        assert_eq!(result.memory_fit, "fits");
    }

    #[test]
    fn refuses_to_guess_unknown_dtype() {
        let memory = MachineMemory {
            total: Some(16 * GIB),
            available: Some(12 * GIB),
            budget: Some(10 * GIB),
            source: "test",
        };
        let result = evaluate(model("CUSTOM", 1_000_000_000), memory);
        assert_eq!(result.weight_bytes, None);
        assert_eq!(result.memory_fit, "unknown");
        assert_eq!(result.confidence, "low");
    }

    #[test]
    fn ranks_supported_fitting_models_first() {
        let memory = MachineMemory {
            total: Some(8 * GIB),
            available: Some(6 * GIB),
            budget: Some(5 * GIB),
            source: "test",
        };
        let fitting = evaluate(model("BF16", 500_000_000), memory);
        let oversized = evaluate(model("BF16", 8_000_000_000), memory);
        assert_eq!(compare(&fitting, &oversized), Ordering::Less);
    }
}
