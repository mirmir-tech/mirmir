mod estimate;
mod variants;

use std::cmp::Ordering;

pub use estimate::{Confidence, MemoryFit, admission_status, rank};
use libmir::{AdmissionStatus, WeightContainer};
use sysinfo::System;
pub use variants::{CatalogVariant, VariantSize, variants};

use super::hub::{Features, HubModel};
use crate::model::format::{Ecosystem, ModelEncoding, ModelFormat};

const GIB: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct MachineMemory {
    pub total: Option<u64>,
    pub available: Option<u64>,
    pub budget: Option<u64>,
    pub source: &'static str,
}

#[derive(Debug, Clone)]
pub struct CatalogModel {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub gated: bool,
    pub model_class: String,
    pub compatibility: AdmissionStatus,
    pub memory_fit: MemoryFit,
    pub weight_bytes: Option<u64>,
    pub required_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub confidence: Confidence,
    pub reason: String,
    pub downloaded: bool,
    pub local_source: &'static str,
    pub ecosystem: Ecosystem,
    pub container: Option<WeightContainer>,
    pub encoding: ModelEncoding,
    pub metal_compatibility: AdmissionStatus,
    pub cuda_compatibility: AdmissionStatus,
    pub preflight_bytes: Option<u64>,
    pub preflight_error: Option<String>,
    pub features: Features,
    /// A GGUF-only repository's quantizations, fewest bits first; empty for
    /// other repositories.
    pub variants: Vec<CatalogVariant>,
    /// The variant a download without one takes; the row's size and fit
    /// are its own.
    pub default_variant: Option<String>,
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
    let facts = model.facts();
    let format = ModelFormat::remote(model.library_name.as_deref(), facts);
    let gguf = model.gguf_variants();
    // Variants only where a pull takes one: a repository with a base
    // model's `SafeTensors` weights beside its GGUF files downloads those.
    let (variants, default_variant) =
        if facts.container == Some(WeightContainer::Gguf) && gguf.gguf_only() {
            let parameters = model.gguf.as_ref().and_then(|gguf| gguf.total);
            variants::variants(&gguf, parameters, memory)
        } else {
            (Vec::new(), None)
        };
    let architecture = model.gguf.as_ref().and_then(|gguf| gguf.architecture.as_deref());
    let compatibility = match facts.container {
        Some(WeightContainer::Gguf) => variants::compatibility(architecture, &gguf),
        Some(WeightContainer::SafeTensors) | None => {
            compatibility(&model, facts.container, gguf.has_safetensors_base())
        },
    };
    let chosen = variants.iter().find(|variant| Some(&variant.name) == default_variant.as_ref());
    let weight_bytes = chosen.map_or_else(|| weights(&model), |variant| variant.size.bytes());
    let required_bytes = weight_bytes.map(required);
    let memory_fit = MemoryFit::of(required_bytes, memory.budget);
    let confidence = if required_bytes.is_some() && memory.budget.is_some() {
        Confidence::Medium
    } else {
        Confidence::Low
    };
    let reason = estimate::reason(compatibility, memory_fit);
    let gated = model.is_gated();
    let identity = format!("{} {model_class}", model.id).to_ascii_lowercase();
    let generation = !identity.contains("embedding") && !identity.contains("rerank");
    let qwen3 = identity.contains("qwen3");
    let tagged = facts.features;
    let tool_use = tagged.tool_use
        || generation && (qwen3 || identity.contains("qwen2.5") && identity.contains("instruct"));
    let thinking = tagged.thinking
        || generation && (qwen3 || identity.contains("qwq") || identity.contains("deepseek-r1"));
    let vision = tagged.vision || model_class.to_ascii_lowercase().contains("vision");
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
        ecosystem: format.ecosystem,
        container: format.container,
        encoding: format.encoding,
        metal_compatibility: format.metal,
        cuda_compatibility: format.cuda,
        preflight_bytes: None,
        preflight_error: None,
        features: Features { tool_use, thinking, vision },
        variants,
        default_variant,
    }
}

pub fn compare(left: &CatalogModel, right: &CatalogModel) -> Ordering {
    rank(left.compatibility, left.memory_fit)
        .cmp(&rank(right.compatibility, right.memory_fit))
        .then_with(|| right.downloads.cmp(&left.downloads))
        .then_with(|| left.id.cmp(&right.id))
}

/// A `SafeTensors` repository's admission waits for its headers.
const fn compatibility(
    model: &HubModel,
    container: Option<WeightContainer>,
    safetensors_base: bool,
) -> AdmissionStatus {
    match container {
        Some(WeightContainer::Gguf) => AdmissionStatus::Unknown,
        Some(WeightContainer::SafeTensors) if model.safetensors.is_some() || safetensors_base => {
            AdmissionStatus::Unknown
        },
        Some(WeightContainer::SafeTensors) | None => AdmissionStatus::Unsupported,
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

#[cfg(test)]
mod tests;
