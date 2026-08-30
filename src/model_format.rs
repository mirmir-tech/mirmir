use std::{fs, path::Path};

use libmir::{BackendTarget, ModelDescriptor};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ModelFormat {
    pub ecosystem: String,
    pub container: String,
    pub encoding: String,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
}

impl ModelFormat {
    pub fn remote(library: Option<&str>, tags: &[String], has_safetensors: bool) -> Self {
        let gguf = tagged(tags, "gguf");
        let container = if gguf {
            "GGUF"
        } else if has_safetensors || tagged(tags, "safetensors") {
            "SafeTensors"
        } else {
            "Unknown"
        };
        let ecosystem = if gguf {
            "GGML"
        } else {
            library.unwrap_or("Unknown")
        };
        let encoding = remote_encoding(tags, gguf);
        let compatibility = if gguf {
            "unsupported"
        } else {
            "unknown"
        };
        Self {
            ecosystem: display_name(ecosystem),
            container: container.into(),
            encoding,
            metal_compatibility: compatibility.into(),
            cuda_compatibility: compatibility.into(),
        }
    }

    pub fn local(path: &Path, config: &Value, descriptor: Option<&ModelDescriptor>) -> Self {
        if has_extension(path, "gguf") {
            return Self {
                ecosystem: "GGML".into(),
                container: "GGUF".into(),
                encoding: "Unknown GGUF".into(),
                metal_compatibility: "unsupported".into(),
                cuda_compatibility: "unsupported".into(),
            };
        }
        let container = if has_extension(path, "safetensors") {
            "SafeTensors"
        } else {
            "Unknown"
        };
        let encoding = descriptor.map_or_else(
            || metadata_encoding(config),
            |descriptor| descriptor.checkpoint_encoding().label(),
        );
        let ecosystem = if config.get("quantization").is_some() || encoding.contains("MLX affine") {
            "MLX"
        } else {
            "Transformers"
        };
        let (metal, cuda) = descriptor.map_or_else(
            || container_compatibility(container),
            |descriptor| {
                (
                    descriptor.admission(BackendTarget::Metal).status.as_str(),
                    descriptor.admission(BackendTarget::Cuda).status.as_str(),
                )
            },
        );
        Self {
            ecosystem: ecosystem.into(),
            container: container.into(),
            encoding,
            metal_compatibility: metal.into(),
            cuda_compatibility: cuda.into(),
        }
    }

    #[must_use]
    pub fn legacy_library(&self) -> String {
        if self.container == "GGUF" {
            self.container.clone()
        } else {
            self.ecosystem.clone()
        }
    }
}

fn metadata_encoding(config: &Value) -> String {
    let quantization = config.get("quantization_config").or_else(|| config.get("quantization"));
    let Some(quantization) = quantization else {
        return config
            .get("dtype")
            .or_else(|| config.get("torch_dtype"))
            .and_then(Value::as_str)
            .map_or_else(|| "Unknown".into(), |dtype| format!("Dense {dtype}"));
    };
    if quantization.get("quant_method").and_then(Value::as_str) == Some("bitsandbytes")
        && let Some(kind) = quantization.get("bnb_4bit_quant_type").and_then(Value::as_str)
    {
        return format!("bitsandbytes {}", kind.to_ascii_uppercase());
    }
    for key in ["quant_algo", "quant_method", "mode"] {
        if let Some(value) = quantization.get(key).and_then(Value::as_str) {
            return display_name(value);
        }
    }
    quantization
        .get("bits")
        .and_then(Value::as_u64)
        .map_or_else(|| "Quantized".into(), |bits| format!("{bits}-bit"))
}

fn container_compatibility(container: &str) -> (&'static str, &'static str) {
    if container != "SafeTensors" {
        return ("unsupported", "unsupported");
    }
    ("unknown", "unknown")
}

fn remote_encoding(tags: &[String], gguf: bool) -> String {
    if gguf {
        return "GGUF quantization".into();
    }
    for (tag, label) in [
        ("nvfp4", "NVFP4"),
        ("mxfp4", "MXFP4"),
        ("awq", "AWQ"),
        ("gptq", "GPTQ"),
        ("bitsandbytes", "bitsandbytes"),
        ("fp8", "FP8"),
    ] {
        if tagged(tags, tag) {
            return label.into();
        }
    }
    "Unknown".into()
}

fn tagged(tags: &[String], expected: &str) -> bool {
    tags.iter().any(|tag| tag.eq_ignore_ascii_case(expected))
}

fn has_extension(path: &Path, extension: &str) -> bool {
    fs::read_dir(path).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
    })
}

fn display_name(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "mlx" => "MLX".into(),
        "transformers" => "Transformers".into(),
        "ggml" => "GGML".into(),
        "nvfp4" => "NVFP4".into(),
        "mxfp4" => "MXFP4".into(),
        _ => value.to_owned(),
    }
}

#[cfg(test)]
mod tests {
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
        let format = ModelFormat::remote(
            Some("transformers"),
            &["safetensors".into(), "nvfp4".into()],
            true,
        );
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
}
