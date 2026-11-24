use libmir::EncodingKind;

use super::*;
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
    assert_eq!(format.encoding, "NVFP4");
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
    assert_eq!(gguf.container, Some(WeightContainer::Gguf));
    assert_eq!(gguf.ecosystem, Ecosystem::Ggml);
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
    let config = serde_json::json!({
        "quantization_config": {
            "quant_method": "bitsandbytes",
            "bnb_4bit_quant_type": "nf4"
        }
    });
    assert_eq!(metadata_encoding(&config), "bitsandbytes NF4");
}

#[test]
fn describes_a_gguf_type_mix_by_share_of_bytes() {
    let mix = gguf::describe(&[
        (libmir::gguf::GgmlType::Q4_K, 745),
        (libmir::gguf::GgmlType::Q6_K, 250),
        (libmir::gguf::GgmlType::F32, 5),
    ]);
    assert_eq!(mix, "Q4_K 74% · Q6_K 25%");
}
