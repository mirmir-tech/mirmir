use libmir::{
    CancellationToken, ChatCompletionRequest, ChatFunctionCall, ChatFunctionDefinition,
    ChatMessage, ChatTool, ChatToolCall,
};
use tonic::Status;

use crate::rpc::proto;

pub(super) fn generate(
    model: &libmir::Model,
    chat: &ChatCompletionRequest,
    image: Option<&[u8]>,
    progress: &mut dyn FnMut(libmir::ProgressEvent),
    token: &mut dyn FnMut(libmir::GenerationToken),
    cancellation: &CancellationToken,
) -> libmir::Result<libmir::GenerationOutput> {
    if let Some(image) = image {
        return model.generate_image_cancellable(chat, image, progress, token, cancellation);
    }
    model.generate_cancellable(chat, progress, token, cancellation)
}

pub(super) fn chat_request(
    request: &proto::GenerateRequest,
) -> Result<ChatCompletionRequest, Status> {
    Ok(ChatCompletionRequest {
        model: request.model.clone(),
        messages: messages(request)?,
        tools: request.tools.iter().map(tool).collect::<Result<_, _>>()?,
        tool_choice: request
            .tool_choice_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| json_status(&error))?,
        stream: true,
        max_tokens: request.max_tokens.map(usize::try_from).transpose().map_err(integer_status)?,
        temperature: request.temperature,
        top_p: request.top_p,
        top_k: request.top_k.map(usize::try_from).transpose().map_err(integer_status)?,
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
            parameters: serde_json::from_str(&function.parameters_json)
                .map_err(|error| json_status(&error))?,
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
            arguments: serde_json::from_str(&function.arguments_json)
                .map_err(|error| json_status(&error))?,
        },
    })
}

fn json_status(error: &serde_json::Error) -> Status {
    Status::invalid_argument(error.to_string())
}

fn integer_status(error: std::num::TryFromIntError) -> Status {
    Status::invalid_argument(error.to_string())
}
