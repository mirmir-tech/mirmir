use libmir::{CancellationToken, ChatCompletionRequest, ChatMessage};
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
        messages: messages(request),
        stream: true,
        max_tokens: request.max_tokens.map(usize::try_from).transpose().map_err(integer_status)?,
        temperature: request.temperature,
        top_p: request.top_p,
        top_k: request.top_k.map(usize::try_from).transpose().map_err(integer_status)?,
        repetition_penalty: request.repetition_penalty,
        seed: request.seed,
    })
}

fn messages(request: &proto::GenerateRequest) -> Vec<ChatMessage> {
    if request.messages.is_empty() {
        return vec![ChatMessage {
            role: "user".to_owned(),
            content: request.prompt.clone(),
            reasoning_content: None,
        }];
    }
    request
        .messages
        .iter()
        .map(|message| ChatMessage {
            role: message.role.clone(),
            content: message.content.clone(),
            reasoning_content: message.reasoning_content.clone(),
        })
        .collect()
}

fn integer_status(error: std::num::TryFromIntError) -> Status {
    Status::invalid_argument(error.to_string())
}
