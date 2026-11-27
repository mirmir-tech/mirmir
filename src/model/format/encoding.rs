//! What a model's weights are stored as, as far as it is known: typed
//! through the model and catalog layers, spelled by [`ModelEncoding::label`]
//! only where the protocols carry it.

use libmir::{CheckpointEncoding, EncodingKind, gguf::GgmlType};
use serde_json::Value;

use super::Ecosystem;
use crate::catalog::encoding_named;

/// The `config.json` key MLX records its quantization under; Transformers
/// uses `quantization_config`.
pub(super) const MLX_QUANTIZATION: &str = "quantization";
const QUANTIZATION_CONFIG: &str = "quantization_config";

/// Types below this share of a GGUF payload (norms, biases) are left out.
const MIN_PERCENT: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelEncoding {
    /// Nothing names it.
    Unknown,
    /// The family a Hub tag names, before the weights are inspected.
    Family(EncodingKind),
    /// The encodings libmir binds the checkpoint's weights with.
    Checkpoint(CheckpointEncoding),
    /// A GGUF text model's GGML types by payload bytes, largest first.
    Ggml(Vec<(GgmlType, u64)>),
    /// A directory of GGUF files none of which is a text model.
    NoGgufTextModel,
    /// A GGUF model whose header could not be read, and why.
    UnreadableGguf(String),
    /// What `config.json` declares, for weights libmir could not inspect.
    Declared(Declared),
}

/// The weights a `config.json` declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declared {
    /// Unquantized, of this dtype.
    Dense(String),
    /// bitsandbytes 4-bit of this type (`nf4`, `fp4`).
    BitsAndBytes4Bit(String),
    /// Quantized by the method or library the configuration names, as it
    /// spells it (`awq`, `mlx`, `compressed-tensors`).
    Method(String),
    /// Quantized to this many bits by an unnamed method.
    Bits(u64),
    /// Quantized, naming neither method nor bits.
    Quantized,
}

impl ModelEncoding {
    /// What `config` declares; [`Self::Unknown`] when it declares nothing.
    #[must_use]
    pub fn declared(config: &Value) -> Self {
        let quantization = config.get(QUANTIZATION_CONFIG).or_else(|| config.get(MLX_QUANTIZATION));
        let Some(quantization) = quantization else {
            return config
                .get("dtype")
                .or_else(|| config.get("torch_dtype"))
                .and_then(Value::as_str)
                .map_or(Self::Unknown, |dtype| Self::Declared(Declared::Dense(dtype.to_owned())));
        };
        let text = |key: &str| quantization.get(key).and_then(Value::as_str);
        let declared = if text("quant_method") == Some("bitsandbytes")
            && let Some(kind) = text("bnb_4bit_quant_type")
        {
            Declared::BitsAndBytes4Bit(kind.to_owned())
        } else if let Some(method) =
            ["quant_algo", "quant_method", "mode"].into_iter().find_map(text)
        {
            Declared::Method(method.to_owned())
        } else {
            quantization
                .get("bits")
                .and_then(Value::as_u64)
                .map_or(Declared::Quantized, Declared::Bits)
        };
        Self::Declared(declared)
    }

    /// Whether any of the weights is of the `kind` family.
    #[must_use]
    pub fn contains(&self, kind: EncodingKind) -> bool {
        match self {
            Self::Family(family) => *family == kind,
            Self::Checkpoint(encoding) => {
                encoding.weights.iter().any(|weights| weights.kind() == kind)
            },
            Self::Ggml(_) => kind == EncodingKind::Ggml,
            Self::Unknown | Self::NoGgufTextModel | Self::UnreadableGguf(_) | Self::Declared(_) => {
                false
            },
        }
    }

    /// The label, unless nothing names the encoding.
    #[must_use]
    pub fn known_label(&self) -> Option<String> {
        (*self != Self::Unknown).then(|| self.label())
    }

    /// The protocol spelling: `Q4_K 74% · Q6_K 25%` for a GGUF type mix.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Unknown => "Unknown".into(),
            Self::Family(kind) => kind.label().into(),
            Self::Checkpoint(encoding) => encoding.label(),
            Self::Ggml(type_bytes) => describe(type_bytes),
            Self::NoGgufTextModel => "Unknown GGUF".into(),
            Self::UnreadableGguf(error) => format!("Unreadable GGUF: {error}"),
            Self::Declared(Declared::Dense(dtype)) => format!("Dense {dtype}"),
            Self::Declared(Declared::BitsAndBytes4Bit(kind)) => {
                format!("{} {}", EncodingKind::BitsAndBytes4Bit.label(), kind.to_ascii_uppercase())
            },
            Self::Declared(Declared::Method(method)) => encoding_named(method).map_or_else(
                || Ecosystem::of_library(Some(method)).name().to_owned(),
                |kind| kind.label().to_owned(),
            ),
            Self::Declared(Declared::Bits(bits)) => format!("{bits}-bit"),
            Self::Declared(Declared::Quantized) => "Quantized".into(),
        }
    }
}

fn describe(type_bytes: &[(GgmlType, u64)]) -> String {
    let total: u64 = type_bytes.iter().map(|entry| entry.1).sum();
    if total == 0 {
        return "Empty GGUF".into();
    }
    type_bytes
        .iter()
        .map(|&(kind, bytes)| (kind, bytes.saturating_mul(100) / total))
        .filter(|&(_, percent)| percent >= MIN_PERCENT)
        .map(|(kind, percent)| format!("{kind} {percent}%"))
        .collect::<Vec<_>>()
        .join(" · ")
}
