use serde::Serialize;

use crate::rpc::proto;

#[derive(Serialize)]
pub(super) struct TaskCapabilities {
    max_input_tokens: u64,
    embedding: Option<EmbeddingCapabilities>,
    rerank: Option<RerankCapabilities>,
}

#[derive(Serialize)]
struct EmbeddingCapabilities {
    native_dimensions: u64,
    pooling: String,
    normalized: bool,
    prompt_names: Vec<String>,
    default_prompt: Option<String>,
    includes_prompt: bool,
}

#[derive(Serialize)]
struct RerankCapabilities {
    labels: u64,
    pooling: String,
    raw_scores: bool,
}

impl From<proto::ModelTaskCapabilities> for TaskCapabilities {
    fn from(value: proto::ModelTaskCapabilities) -> Self {
        Self {
            max_input_tokens: value.max_input_tokens,
            embedding: value.embedding.map(EmbeddingCapabilities::from),
            rerank: value.rerank.map(RerankCapabilities::from),
        }
    }
}

impl From<proto::EmbeddingCapabilities> for EmbeddingCapabilities {
    fn from(value: proto::EmbeddingCapabilities) -> Self {
        Self {
            native_dimensions: value.native_dimensions,
            pooling: value.pooling,
            normalized: value.normalized,
            prompt_names: value.prompt_names,
            default_prompt: value.default_prompt,
            includes_prompt: value.includes_prompt,
        }
    }
}

impl From<proto::RerankCapabilities> for RerankCapabilities {
    fn from(value: proto::RerankCapabilities) -> Self {
        Self {
            labels: value.labels,
            pooling: value.pooling,
            raw_scores: value.raw_scores,
        }
    }
}
