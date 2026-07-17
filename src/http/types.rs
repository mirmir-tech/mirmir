use serde::{Deserialize, Serialize};

use super::error::ApiError;
use crate::rpc::proto;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u64>,
    pub repetition_penalty: Option<f32>,
    pub seed: Option<u64>,
    pub n: Option<u32>,
    pub stream_options: Option<StreamOptions>,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, alias = "reasoning")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StreamOptions {
    #[serde(default)]
    pub include_usage: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub server_version: String,
    pub protocol_version: String,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: &'static str,
    pub data: Vec<Model>,
}

#[derive(Debug, Serialize)]
pub struct Model {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub owned_by: &'static str,
}

#[derive(Debug, Serialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: ResponseMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct ResponseMessage {
    pub role: &'static str,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    #[serde(rename = "prompt_tokens")]
    pub prompt: u64,
    #[serde(rename = "completion_tokens")]
    pub completion: u64,
    #[serde(rename = "total_tokens")]
    pub total: u64,
}

impl ChatRequest {
    pub fn into_proto(self) -> Result<proto::GenerateRequest, ApiError> {
        if self.model.trim().is_empty() {
            return Err(ApiError::bad_request("model cannot be empty"));
        }
        if self.messages.is_empty() {
            return Err(ApiError::bad_request("messages cannot be empty"));
        }
        if self.n.unwrap_or(1) != 1 {
            return Err(ApiError::bad_request("only n=1 is supported"));
        }
        let max_tokens = match (self.max_tokens, self.max_completion_tokens) {
            (Some(left), Some(right)) if left != right => {
                return Err(ApiError::bad_request("max_tokens and max_completion_tokens disagree"));
            },
            (left, right) => left.or(right),
        };
        Ok(proto::GenerateRequest {
            model: self.model,
            prompt: String::new(),
            max_tokens,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            repetition_penalty: self.repetition_penalty,
            seed: self.seed,
            messages: self
                .messages
                .into_iter()
                .map(|message| proto::ChatMessageInput {
                    role: message.role,
                    content: message.content,
                    reasoning_content: message.reasoning_content,
                })
                .collect(),
        })
    }
}

impl Usage {
    pub const fn from_completion(completion: &proto::Completion) -> Self {
        Self {
            prompt: completion.prompt_tokens,
            completion: completion.completion_tokens,
            total: completion.prompt_tokens.saturating_add(completion.completion_tokens),
        }
    }
}

impl CompletionResponse {
    pub fn new(id: String, created: u64, model: String, completion: proto::Completion) -> Self {
        let usage = Usage::from_completion(&completion);
        Self {
            id,
            object: "chat.completion",
            created,
            model,
            choices: vec![Choice {
                index: 0,
                message: ResponseMessage {
                    role: "assistant",
                    content: completion.text,
                    reasoning_content: (!completion.reasoning.is_empty())
                        .then_some(completion.reasoning),
                },
                finish_reason: completion.finish_reason,
            }],
            usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_reasoning_separately_from_final_content() {
        let response = CompletionResponse::new(
            "id".into(),
            1,
            "model".into(),
            proto::Completion {
                text: "answer".into(),
                reasoning: "draft".into(),
                ..Default::default()
            },
        );
        let value = serde_json::to_value(response).expect("serializable response");
        assert_eq!(value["choices"][0]["message"]["content"], "answer");
        assert_eq!(value["choices"][0]["message"]["reasoning_content"], "draft");
    }
}
