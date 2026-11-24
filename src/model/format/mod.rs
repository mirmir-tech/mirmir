mod ecosystem;
mod gguf;

use std::path::Path;

pub use ecosystem::Ecosystem;
use libmir::{AdmissionStatus, BackendTarget, ModelDescriptor, WeightContainer};
use serde_json::Value;

use crate::catalog::HubFacts;

#[derive(Clone, Debug)]
pub struct ModelFormat {
    pub ecosystem: Ecosystem,
    pub container: Option<WeightContainer>,
    pub encoding: String,
    pub metal: AdmissionStatus,
    pub cuda: AdmissionStatus,
}

impl ModelFormat {
    /// A Hub model before download: its weights are uninspected, so neither
    /// backend's admission is known yet.
    pub fn remote(library: Option<&str>, facts: HubFacts) -> Self {
        let ecosystem = match facts.container {
            Some(WeightContainer::Gguf) => Ecosystem::Ggml,
            Some(WeightContainer::SafeTensors) | None => Ecosystem::of_library(library),
        };
        Self {
            ecosystem,
            container: facts.container,
            encoding: facts.encoding.map_or("Unknown", libmir::EncodingKind::label).into(),
            metal: AdmissionStatus::Unknown,
            cuda: AdmissionStatus::Unknown,
        }
    }

    /// A downloaded model; `descriptor` is `None` when libmir could not
    /// inspect the directory, whose admission is then unknown when it holds
    /// weights libmir reads.
    pub fn local(path: &Path, config: &Value, descriptor: Option<&ModelDescriptor>) -> Self {
        let container = descriptor.map_or_else(
            || WeightContainer::detect(path).ok().flatten(),
            ModelDescriptor::weight_container,
        );
        let (ecosystem, encoding) = match container {
            Some(WeightContainer::Gguf) => (Ecosystem::Ggml, gguf::encoding(path)),
            Some(WeightContainer::SafeTensors) | None => {
                let encoding = descriptor.map_or_else(
                    || metadata_encoding(config),
                    |descriptor| descriptor.checkpoint_encoding().label(),
                );
                let mlx = config.get("quantization").is_some() || encoding.contains("MLX affine");
                (
                    if mlx {
                        Ecosystem::Mlx
                    } else {
                        Ecosystem::Transformers
                    },
                    encoding,
                )
            },
        };
        let (metal, cuda) = descriptor.map_or_else(
            || {
                let status =
                    container.map_or(AdmissionStatus::Unsupported, |_| AdmissionStatus::Unknown);
                (status, status)
            },
            |descriptor| {
                (
                    descriptor.admission(BackendTarget::Metal).status,
                    descriptor.admission(BackendTarget::Cuda).status,
                )
            },
        );
        Self {
            ecosystem,
            container,
            encoding,
            metal,
            cuda,
        }
    }
}

/// The protocol spelling of a weight container.
#[must_use]
pub const fn container_name(container: Option<WeightContainer>) -> &'static str {
    match container {
        Some(WeightContainer::Gguf) => "GGUF",
        Some(WeightContainer::SafeTensors) => "SafeTensors",
        None => "Unknown",
    }
}

/// The legacy `library` protocol field: the container for GGUF, else the
/// ecosystem.
#[must_use]
pub fn legacy_library(container: Option<WeightContainer>, ecosystem: &Ecosystem) -> &str {
    match container {
        Some(WeightContainer::Gguf) => container_name(container),
        Some(WeightContainer::SafeTensors) | None => ecosystem.name(),
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
mod tests;
