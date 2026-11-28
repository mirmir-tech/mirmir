use libmir::{CheckpointEncoding, EncodingKind, WeightEncoding};
use serde_json::json;

use super::{encoding::Declared, *};
use crate::catalog::HubFacts;

#[test]
fn remote_encoding_does_not_claim_backend_support() {
    let facts = HubFacts {
        container: Some(WeightContainer::SafeTensors),
        encoding: Some(EncodingKind::NvFp4),
        ..HubFacts::default()
    };
    let format = ModelFormat::remote(Some("transformers"), facts);
    assert_eq!(format.ecosystem, Ecosystem::Transformers);
    assert_eq!(format.encoding, ModelEncoding::Family(EncodingKind::NvFp4));
    assert_eq!(format.encoding.label(), "NVFP4");
    assert_eq!(format.metal, AdmissionStatus::Unknown);
    assert_eq!(format.cuda, AdmissionStatus::Unknown);
}

/// Without a descriptor, weights libmir reads may still load; anything
/// else cannot.
#[test]
fn an_uninspected_directory_is_unknown_only_with_weights() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-format-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let without = ModelFormat::local(&root, &Value::Null, None);
    std::fs::write(root.join("model-Q4_K_M.gguf"), [])?;
    let gguf = ModelFormat::local(&root, &Value::Null, None);
    std::fs::remove_dir_all(&root)?;
    assert_eq!((without.container, without.metal), (None, AdmissionStatus::Unsupported));
    assert_eq!(without.encoding, ModelEncoding::Unknown);
    assert_eq!(gguf.container, Some(WeightContainer::Gguf));
    assert_eq!(gguf.ecosystem, Ecosystem::Ggml);
    assert!(matches!(gguf.encoding, ModelEncoding::UnreadableGguf(_)));
    assert_eq!((gguf.metal, gguf.cuda), (AdmissionStatus::Unknown, AdmissionStatus::Unknown));
    Ok(())
}

#[test]
fn the_legacy_library_names_gguf_by_its_container() {
    assert_eq!(legacy_library(Some(WeightContainer::Gguf), &Ecosystem::Ggml), "GGUF");
    assert_eq!(legacy_library(Some(WeightContainer::SafeTensors), &Ecosystem::Mlx), "MLX");
    assert_eq!(legacy_library(None, &Ecosystem::Library("diffusers".into())), "diffusers");
}

#[test]
fn bitsandbytes_metadata_preserves_the_four_bit_type() {
    let config = json!({
        "quantization_config": {
            "quant_method": "bitsandbytes",
            "bnb_4bit_quant_type": "nf4"
        }
    });
    let encoding = ModelEncoding::declared(&config);
    assert_eq!(encoding, ModelEncoding::Declared(Declared::BitsAndBytes4Bit("nf4".into())));
    assert_eq!(encoding.label(), "bitsandbytes NF4");
}

/// A declared method is spelled as the encoding family or the ecosystem it
/// names, else as the configuration spells it.
#[test]
fn declared_methods_take_the_family_or_ecosystem_spelling() {
    let method = |name: &str| {
        ModelEncoding::declared(&json!({ "quantization_config": { "quant_method": name } })).label()
    };
    assert_eq!(method("nvfp4"), "NVFP4");
    assert_eq!(method("mlx"), "MLX");
    assert_eq!(method("compressed-tensors"), "compressed-tensors");
    let dense = ModelEncoding::declared(&json!({ "torch_dtype": "bfloat16" }));
    assert_eq!(dense.label(), "Dense bfloat16");
    let bits = ModelEncoding::declared(&json!({ "quantization": { "bits": 4 } }));
    assert_eq!(bits, ModelEncoding::Declared(Declared::Bits(4)));
}

/// MLX is told by its `quantization` key or grouped affine weights, never by
/// how an encoding is spelled.
#[test]
fn mlx_is_decided_from_the_encoding_family() {
    let dense = ModelEncoding::Checkpoint(CheckpointEncoding {
        weights: vec![WeightEncoding::Dense { dtype: libmir::DenseDType::Bf16 }],
    });
    assert_eq!(safetensors_ecosystem(&json!({}), &dense), Ecosystem::Transformers);
    let mlx = json!({ "quantization": { "group_size": 64, "bits": 4 } });
    assert_eq!(safetensors_ecosystem(&mlx, &dense), Ecosystem::Mlx);
    let affine = ModelEncoding::Family(EncodingKind::Affine);
    assert!(affine.contains(EncodingKind::Affine) && !dense.contains(EncodingKind::Affine));
    assert_eq!(safetensors_ecosystem(&json!({}), &affine), Ecosystem::Mlx);
}

#[test]
fn describes_a_gguf_type_mix_by_share_of_bytes() {
    let mix = ModelEncoding::Ggml(vec![
        (libmir::gguf::GgmlType::Q4_K, 745),
        (libmir::gguf::GgmlType::Q6_K, 250),
        (libmir::gguf::GgmlType::F32, 5),
    ]);
    assert_eq!(mix.label(), "Q4_K 74% · Q6_K 25%");
}
