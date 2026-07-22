use std::{collections::BTreeSet, fs, path::Path};

use libmir::{
    ModelDescriptor,
    models::weights::{BlockFormat, TensorStorage},
};
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
        let encoding = descriptor.map_or_else(|| metadata_encoding(config), binding_encoding);
        let ecosystem = if config.get("quantization").is_some() || encoding.contains("MLX affine") {
            "MLX"
        } else {
            "Transformers"
        };
        let (metal, cuda) = compatibility(&encoding, container);
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

fn binding_encoding(descriptor: &ModelDescriptor) -> String {
    let Some(execution) = descriptor.execution() else {
        return metadata_quantization(descriptor.metadata().quantization.clone());
    };
    let mut encodings = BTreeSet::new();
    for binding in &execution.bindings.tensors {
        let value = match &binding.storage {
            TensorStorage::Dense { dtype, .. } => format!("Dense {dtype}"),
            TensorStorage::AffineQuantized { bits, group_size, .. } => format!(
                "MLX affine {} G{}",
                bits.map_or_else(|| "?".into(), |value| value.to_string()),
                group_size.map_or_else(|| "?".into(), |value| value.to_string())
            ),
            TensorStorage::PackedInt8 { .. } => "Packed INT8".into(),
            TensorStorage::BlockQuantized { format, .. } => match format {
                BlockFormat::MxFp4 => "MXFP4".into(),
                BlockFormat::NvFp4 => "NVFP4".into(),
            },
            TensorStorage::Auxiliary { .. } => continue,
        };
        let _inserted = encodings.insert(value);
    }
    if encodings.is_empty() {
        "Unknown".into()
    } else {
        encodings.into_iter().collect::<Vec<_>>().join(" + ")
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

fn metadata_quantization(value: libmir::foundation::model::Quantization) -> String {
    use libmir::foundation::model::Quantization;
    match value {
        Quantization::None => "Unknown".into(),
        Quantization::F16 => "Dense F16".into(),
        Quantization::Bf16 => "Dense BF16".into(),
        Quantization::Int8 => "INT8".into(),
        Quantization::Int4 => "INT4".into(),
        Quantization::Fp8 => "FP8".into(),
        Quantization::NvFp4 => "NVFP4".into(),
        Quantization::MxFp4 => "MXFP4".into(),
        Quantization::Custom(name) => name,
    }
}

fn compatibility(encoding: &str, container: &str) -> (&'static str, &'static str) {
    if container != "SafeTensors" {
        return ("unsupported", "unsupported");
    }
    if encoding.contains("NVFP4") || encoding.contains("Packed INT8") {
        return ("unsupported", "partial");
    }
    if encoding.contains("MXFP4")
        || encoding.contains("F16")
        || encoding.contains("F32")
        || encoding.contains("BF16")
        || encoding.contains("MLX affine")
    {
        return ("partial", "partial");
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
}
