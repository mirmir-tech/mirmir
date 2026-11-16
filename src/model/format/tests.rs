use super::*;

#[test]
fn gguf_is_not_admitted_by_native_backends() {
    let format = ModelFormat::remote(None, &["gguf".into()], false);
    assert_eq!(format.ecosystem, "GGML");
    assert_eq!(format.container, "GGUF");
    assert_eq!(format.metal_compatibility, "unsupported");
    assert_eq!(format.cuda_compatibility, "unsupported");
}

#[test]
fn remote_encoding_does_not_claim_backend_support() {
    let format =
        ModelFormat::remote(Some("transformers"), &["safetensors".into(), "nvfp4".into()], true);
    assert_eq!(format.encoding, "NVFP4");
    assert_eq!(format.metal_compatibility, "unknown");
    assert_eq!(format.cuda_compatibility, "unknown");
}

#[test]
fn safetensors_without_a_descriptor_stays_unknown() {
    assert_eq!(container_compatibility("SafeTensors"), ("unknown", "unknown"));
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
