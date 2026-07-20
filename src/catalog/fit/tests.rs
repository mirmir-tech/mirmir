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
