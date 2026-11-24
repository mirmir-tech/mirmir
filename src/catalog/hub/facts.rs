//! What a Hub model's tags say, read once: here Hub tags become libmir types.

use std::collections::HashSet;

use libmir::{EncodingKind, WeightContainer};

use super::HubModel;

/// Chat features a model's tags declare.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Features {
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

/// A Hub model's tags and metadata, folded to types.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HubFacts {
    pub container: Option<WeightContainer>,
    /// The quantization family a tag names, without its parameters.
    pub encoding: Option<EncodingKind>,
    pub features: Features,
}

/// Quantization tags, the first present one naming the encoding.
const ENCODINGS: [(&str, EncodingKind); 6] = [
    ("nvfp4", EncodingKind::NvFp4),
    ("mxfp4", EncodingKind::MxFp4),
    ("awq", EncodingKind::Awq),
    ("gptq", EncodingKind::Gptq),
    ("bitsandbytes", EncodingKind::BitsAndBytes4Bit),
    ("fp8", EncodingKind::Float8),
];
/// The tags naming a repository's weight files.
const GGUF_TAG: &str = "gguf";
const SAFETENSORS_TAG: &str = "safetensors";
const TOOL_USE: [&str; 3] = ["tool-use", "tool-calling", "function-calling"];
const THINKING: [&str; 2] = ["reasoning", "thinking"];
const VISION: [&str; 3] = ["image-text-to-text", "multimodal", "vision"];

/// The encoding family a quantization tag or method names, in any case.
#[must_use]
pub fn encoding_named(name: &str) -> Option<EncodingKind> {
    ENCODINGS
        .into_iter()
        .find(|(tag, _)| tag.eq_ignore_ascii_case(name))
        .map(|(_, kind)| kind)
}

impl HubModel {
    /// Folds the tags' case once; unknown tags are ignored.
    #[must_use]
    pub fn facts(&self) -> HubFacts {
        let tags = self.tags.iter().map(|tag| tag.to_ascii_lowercase()).collect::<HashSet<_>>();
        let has = |tag: &str| tags.contains(tag);
        let container = if has(GGUF_TAG) {
            Some(WeightContainer::Gguf)
        } else if self.safetensors.is_some() || has(SAFETENSORS_TAG) {
            Some(WeightContainer::SafeTensors)
        } else {
            None
        };
        let encoding = match container {
            Some(WeightContainer::Gguf) => Some(EncodingKind::Ggml),
            Some(WeightContainer::SafeTensors) | None => {
                ENCODINGS.into_iter().find(|&(tag, _)| has(tag)).map(|(_, kind)| kind)
            },
        };
        HubFacts {
            container,
            encoding,
            features: Features {
                tool_use: TOOL_USE.into_iter().any(has),
                thinking: THINKING.into_iter().any(has),
                vision: VISION.into_iter().any(has),
            },
        }
    }
}
