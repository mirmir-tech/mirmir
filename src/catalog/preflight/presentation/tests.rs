use std::path::PathBuf;

use libmir::{
    RemoteModelContract,
    models::weights::{TensorCatalog, TensorInfo},
};

use super::*;
use crate::catalog::preflight::RemoteModelMetadata;

#[test]
fn overall_status_uses_the_best_available_backend() {
    assert_eq!(
        best_status(AdmissionStatus::Unsupported, AdmissionStatus::Partial),
        AdmissionStatus::Partial
    );
    assert_eq!(
        best_status(AdmissionStatus::Supported, AdmissionStatus::Partial),
        AdmissionStatus::Supported
    );
    assert_eq!(
        best_status(AdmissionStatus::Unsupported, AdmissionStatus::Unsupported),
        AdmissionStatus::Unsupported
    );
}

#[test]
fn applies_remote_contract_statuses_and_diagnostics() -> crate::error::Result<()> {
    let config = serde_json::json!({
        "architectures": ["MistralForCausalLM"],
        "hidden_size": 32,
        "intermediate_size": 64,
        "num_hidden_layers": 1,
        "num_attention_heads": 4,
        "num_key_value_heads": 2,
        "vocab_size": 64,
        "hidden_act": "silu"
    });
    let catalog = TensorCatalog {
        tensors: vec![TensorInfo {
            name: "model.layers.0.self_attn.q_proj.weight".into(),
            file: PathBuf::from("model.safetensors"),
            dtype: "BF16".into(),
            shape: vec![32, 32],
            data_start: 128,
            data_offsets: [0, 2048],
        }],
    };
    let contract = RemoteModelContract::inspect_generation(&config, &catalog)?;
    let preflight = RemoteHeaderPreflight {
        catalog,
        metadata: RemoteModelMetadata {
            config,
            tokenizer_config: None,
            modules: None,
            pooling: None,
            sentence_transformers: None,
            processor_config: None,
            tokenizer_assets: super::super::tests::tokenizer_assets(),
        },
        contract: Some(contract),
        contract_error: None,
        fetched_bytes: 512,
        files: vec!["model.safetensors".into()],
        revision: "abc123".into(),
    };
    let mut model = CatalogModel {
        compatibility: "unknown",
        ..CatalogModel::default()
    };

    apply(&mut model, &preflight);

    assert_eq!(model.compatibility, "supported");
    assert_eq!(model.metal_compatibility, "supported");
    assert_eq!(model.cuda_compatibility, "supported");
    assert_eq!(model.encoding, "Dense BF16");
    assert_eq!(model.preflight_bytes, Some(512));
    assert_eq!(model.confidence, "high");
    Ok(())
}
