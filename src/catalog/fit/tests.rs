use std::collections::BTreeMap;

use super::*;
use crate::catalog::hub::{HubConfig, SafeTensors};

fn model(dtype: &str, parameters: u64) -> HubModel {
    HubModel {
        id: "Qwen/Test".to_owned(),
        library_name: Some("transformers".to_owned()),
        downloads: 10,
        likes: 2,
        gated: serde_json::Value::Bool(false),
        config: HubConfig {
            architectures: vec!["Qwen2ForCausalLM".to_owned()],
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
    assert_eq!(result.compatibility, "unknown");
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
fn exposes_qwen3_search_capabilities_before_download() {
    let mut candidate = model("BF16", 1_000_000_000);
    candidate.id = "Qwen/Qwen3-8B".to_owned();
    let result = evaluate(
        candidate,
        MachineMemory {
            total: None,
            available: None,
            budget: None,
            source: "test",
        },
    );

    assert_eq!(result.library, "Transformers");
    assert_eq!(result.ecosystem, "Transformers");
    assert_eq!(result.container, "SafeTensors");
    assert_eq!(result.encoding, "Unknown");
    assert_eq!(result.metal_compatibility, "unknown");
    assert_eq!(result.cuda_compatibility, "unknown");
    assert!(result.features.tool_use);
    assert!(result.features.thinking);
    assert!(!result.features.vision);
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

#[test]
fn ranks_inspected_partial_models_before_unknown_models() {
    let mut inspected = CatalogModel {
        compatibility: "partial",
        memory_fit: "fits",
        ..CatalogModel::default()
    };
    let unknown = CatalogModel {
        compatibility: "unknown",
        memory_fit: "fits",
        ..CatalogModel::default()
    };
    inspected.downloads = 1;

    assert_eq!(compare(&inspected, &unknown), Ordering::Less);
}
