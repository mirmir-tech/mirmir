use serde::{Deserialize, Serialize};

use crate::http::error::ApiError;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(in crate::http) enum EmbeddingInput {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub(in crate::http) struct EmbeddingsRequest {
    pub model: String,
    pub input: EmbeddingInput,
    pub dimensions: Option<u64>,
    pub prompt_name: Option<String>,
    pub encoding_format: Option<String>,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct EmbeddingsResponse {
    pub object: &'static str,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: TaskUsage,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct EmbeddingData {
    pub object: &'static str,
    pub embedding: Vec<f32>,
    pub index: usize,
}

#[derive(Debug, Deserialize)]
pub(in crate::http) struct RerankRequest {
    pub model: String,
    pub query: String,
    pub documents: Vec<String>,
    pub max_length: Option<u64>,
    pub top_n: Option<usize>,
    #[serde(default)]
    pub raw_scores: bool,
    #[serde(default)]
    pub return_documents: bool,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct RerankResponse {
    pub results: Vec<RerankData>,
    pub usage: TaskUsage,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct RerankData {
    pub index: u64,
    pub relevance_score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<RerankDocument>,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct RerankDocument {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub(in crate::http) struct TaskUsage {
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}

impl EmbeddingsRequest {
    pub fn into_application(self) -> Result<(String, libmir::EmbeddingRequest), ApiError> {
        if self.encoding_format.as_deref().is_some_and(|format| format != "float") {
            return Err(ApiError::bad_request("only encoding_format=float is supported"));
        }
        let inputs = match self.input {
            EmbeddingInput::One(input) => vec![input],
            EmbeddingInput::Many(inputs) => inputs,
        };
        if self.model.trim().is_empty() || inputs.is_empty() {
            return Err(ApiError::bad_request("model and at least one input are required"));
        }
        let dimensions = self
            .dimensions
            .map(usize::try_from)
            .transpose()
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        Ok((
            self.model,
            libmir::EmbeddingRequest {
                inputs,
                dimensions,
                prompt_name: self.prompt_name,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_openai_single_and_batched_embedding_inputs() {
        let one: EmbeddingsRequest = serde_json::from_value(serde_json::json!({
            "model": "embedding", "input": "one"
        }))
        .expect("single input");
        assert_eq!(one.into_application().expect("valid").1.inputs, ["one"]);

        let many: EmbeddingsRequest = serde_json::from_value(serde_json::json!({
            "model": "embedding", "input": ["one", "two"], "encoding_format": "float"
        }))
        .expect("batch input");
        assert_eq!(many.into_application().expect("valid").1.inputs, ["one", "two"]);
    }
}
