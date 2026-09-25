use serde::{Deserialize, Serialize};

use super::{
    media::MessageContent,
    tools::{RequestTool, RequestToolCall},
};
use crate::application::GenerationResult;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub tools: Vec<RequestTool>,
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default)]
    pub stream: bool,
    /// Total generated-token budget, including reasoning and final content.
    pub max_tokens: Option<u64>,
    pub reasoning: Option<libmir::ReasoningMode>,
    pub chat_template_kwargs: Option<super::reasoning::TemplateOptions>,
    pub thinking_token_budget: Option<serde_json::Value>,
    pub reasoning_budget: Option<serde_json::Value>,
    pub reasoning_effort: Option<serde_json::Value>,
    pub max_completion_tokens: Option<u64>,
    pub min_tokens: Option<u64>,
    pub ignore_eos: Option<bool>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u64>,
    pub repetition_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub seed: Option<u64>,
    pub n: Option<u32>,
    #[serde(default)]
    pub return_token_ids: bool,
    pub stream_options: Option<StreamOptions>,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
    #[serde(default, alias = "reasoning")]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<RequestToolCall>>,
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

impl Usage {
    pub fn from_result(result: &GenerationResult) -> Self {
        let output = &result.output;
        Self {
            prompt: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
            completion: u64::try_from(output.token_ids.len()).unwrap_or(u64::MAX),
            total: u64::try_from(output.prompt_tokens.saturating_add(output.token_ids.len()))
                .unwrap_or(u64::MAX),
        }
    }
}

impl CompletionResponse {
    pub fn new(id: String, created: u64, model: String, result: GenerationResult) -> Self {
        let usage = Usage::from_result(&result);
        let output = result.output;
        Self {
            id,
            object: "chat.completion",
            created,
            model,
            choices: vec![Choice {
                index: 0,
                message: ResponseMessage {
                    role: "assistant",
                    content: output.text,
                    reasoning_content: (!output.reasoning.is_empty()).then_some(output.reasoning),
                    tool_calls: libmir::ToolCall::parse_mistral(&output.tool_calls)
                        .unwrap_or_default()
                        .into_iter()
                        .map(response_tool_call)
                        .collect(),
                },
                finish_reason: output.finish_reason.to_owned(),
            }],
            usage,
        }
    }
}

fn response_tool_call(call: libmir::ToolCall) -> ResponseToolCall {
    ResponseToolCall {
        id: call.id,
        kind: call.kind,
        function: ResponseFunctionCall {
            name: call.function.name,
            arguments: call.function.arguments.to_string(),
        },
    }
}

mod request;
#[cfg(test)]
mod tests;
