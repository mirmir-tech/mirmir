use serde::{Deserialize, Serialize};

use super::{
    error::ApiError,
    media::{MessageContent, messages},
};
use crate::rpc::proto;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub tools: Vec<libmir::ChatTool>,
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u64>,
    pub max_completion_tokens: Option<u64>,
    pub min_tokens: Option<u64>,
    pub ignore_eos: Option<bool>,
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
    pub content: MessageContent,
    #[serde(default, alias = "reasoning")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<libmir::ChatToolCall>>,
    pub tool_call_id: Option<String>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ResponseToolCall>,
}

#[derive(Debug, Serialize)]
pub struct ResponseToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ResponseFunctionCall,
}

#[derive(Debug, Serialize)]
pub struct ResponseFunctionCall {
    pub name: String,
    pub arguments: String,
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
        let (messages, image) = messages(self.messages)?;
        let tools = self.tools.into_iter().map(proto_tool).collect();
        Ok(proto::GenerateRequest {
            model: self.model,
            prompt: String::new(),
            max_tokens,
            min_tokens: self.min_tokens,
            ignore_eos: self.ignore_eos,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            repetition_penalty: self.repetition_penalty,
            seed: self.seed,
            messages,
            image,
            tools,
            tool_choice_json: self.tool_choice.map(|choice| choice.to_string()),
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
                    tool_calls: completion
                        .tool_calls
                        .into_iter()
                        .filter_map(response_tool_call)
                        .collect(),
                },
                finish_reason: completion.finish_reason,
            }],
            usage,
        }
    }
}

fn proto_tool(tool: libmir::ChatTool) -> proto::ChatTool {
    proto::ChatTool {
        r#type: tool.kind,
        function: Some(proto::ChatFunctionDefinition {
            name: tool.function.name,
            description: tool.function.description,
            parameters_json: tool.function.parameters.to_string(),
        }),
    }
}

fn response_tool_call(call: proto::ChatToolCall) -> Option<ResponseToolCall> {
    let function = call.function?;
    Some(ResponseToolCall {
        id: call.id,
        kind: call.r#type,
        function: ResponseFunctionCall {
            name: function.name,
            arguments: function.arguments_json,
        },
    })
}

#[cfg(test)]
mod tests;
