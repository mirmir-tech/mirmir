mod ecosystem;
mod encoding;
mod gguf;

use std::path::Path;

pub use ecosystem::Ecosystem;
pub use encoding::ModelEncoding;
use libmir::{AdmissionStatus, BackendTarget, EncodingKind, ModelDescriptor, WeightContainer};
use serde_json::Value;

use crate::catalog::HubFacts;

#[derive(Clone, Debug)]
pub struct ModelFormat {
    pub ecosystem: Ecosystem,
    pub container: Option<WeightContainer>,
    pub encoding: ModelEncoding,
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
            encoding: facts.encoding.map_or(ModelEncoding::Unknown, ModelEncoding::Family),
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
                    || ModelEncoding::declared(config),
                    |descriptor| ModelEncoding::Checkpoint(descriptor.checkpoint_encoding()),
                );
                (safetensors_ecosystem(config, &encoding), encoding)
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

/// MLX records its `quantization` in `config.json` and stores grouped
/// affine weights; other `SafeTensors` checkpoints are Transformers'.
fn safetensors_ecosystem(config: &Value, encoding: &ModelEncoding) -> Ecosystem {
    if config.get(encoding::MLX_QUANTIZATION).is_some() || encoding.contains(EncodingKind::Affine) {
        Ecosystem::Mlx
    } else {
        Ecosystem::Transformers
    }
}

#[cfg(test)]
mod tests;
