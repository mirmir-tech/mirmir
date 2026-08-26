use libmir::{
    ChatCompletionRequest, ChatFunctionCall, ChatFunctionDefinition, ChatMessage, ChatTool,
    ChatToolCall,
};
use tonic::Status;

use crate::rpc::proto;

pub(super) fn chat_request(
    request: &proto::GenerateRequest,
) -> Result<ChatCompletionRequest, Status> {
    Ok(ChatCompletionRequest {
        model: request.model.clone(),
        messages: messages(request)?,
        tools: request.tools.iter().map(tool).collect::<Result<_, _>>()?,
        tool_choice: crate::rpc::service::status::invalid(
            request.tool_choice_json.as_deref().map(serde_json::from_str).transpose(),
        )?,
        stream: true,
        max_tokens: crate::rpc::service::status::invalid(
            request.max_tokens.map(usize::try_from).transpose(),
        )?,
        min_tokens: crate::rpc::service::status::invalid(
            request.min_tokens.map(usize::try_from).transpose(),
        )?,
        ignore_eos: request.ignore_eos,
        temperature: request.temperature,
        top_p: request.top_p,
        top_k: crate::rpc::service::status::invalid(
            request.top_k.map(usize::try_from).transpose(),
        )?,
        repetition_penalty: request.repetition_penalty,
        seed: request.seed,
    })
}

fn messages(request: &proto::GenerateRequest) -> Result<Vec<ChatMessage>, Status> {
    if request.messages.is_empty() {
        return Ok(vec![ChatMessage {
            role: "user".to_owned(),
            content: request.prompt.clone(),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }]);
    }
    request
        .messages
        .iter()
        .map(|message| {
            Ok(ChatMessage {
                role: message.role.clone(),
                content: message.content.clone(),
                reasoning_content: message.reasoning_content.clone(),
                tool_calls: (!message.tool_calls.is_empty())
                    .then(|| message.tool_calls.iter().map(tool_call).collect())
                    .transpose()?,
                tool_call_id: message.tool_call_id.clone(),
            })
        })
        .collect()
}

fn tool(tool: &proto::ChatTool) -> Result<ChatTool, Status> {
    let function = tool
        .function
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("tool.function is required"))?;
    Ok(ChatTool {
        kind: tool.r#type.clone(),
        function: ChatFunctionDefinition {
            name: function.name.clone(),
            description: function.description.clone(),
            parameters: crate::rpc::service::status::invalid(serde_json::from_str(
                &function.parameters_json,
            ))?,
        },
    })
}

fn tool_call(call: &proto::ChatToolCall) -> Result<ChatToolCall, Status> {
    let function = call
        .function
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("tool_call.function is required"))?;
    Ok(ChatToolCall {
        id: call.id.clone(),
        kind: call.r#type.clone(),
        function: ChatFunctionCall {
            name: function.name.clone(),
            arguments: crate::rpc::service::status::invalid(serde_json::from_str(
                &function.arguments_json,
            ))?,
        },
    })
}
