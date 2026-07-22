use axum::{Json, extract::State};
use libmir::foundation::protocol::{
    ChatChoice, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatToolCall, Usage,
};
use uuid::Uuid;

use crate::{error::ServerError, state::AppState};

pub async fn completion(
    State(state): State<AppState>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, ServerError> {
    if request.messages.is_empty() {
        return Err(ServerError::BadRequest("messages cannot be empty".into()));
    }

    if request.stream {
        return Err(ServerError::BadRequest("use /v1/ws for streaming responses".into()));
    }
    let model = state.model().ok_or_else(|| {
        ServerError::BadRequest("server has no loaded model; configure MODEL or --model".into())
    })?;
    let response_model = request.model.clone();
    let output = tokio::task::spawn_blocking(move || {
        model.generate(&request, &mut |_event| {}, &mut |_token| {})
    })
    .await??;
    let completion_tokens = output.token_ids.len();
    let total_tokens = output.prompt_tokens.saturating_add(completion_tokens);
    let response = ChatCompletionResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion".into(),
        model: response_model,
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".into(),
                content: output.text,
                reasoning_content: (!output.reasoning.is_empty()).then_some(output.reasoning),
                tool_calls: (!output.tool_calls.is_empty())
                    .then(|| ChatToolCall::parse_mistral(&output.tool_calls).unwrap_or_default()),
                tool_call_id: None,
            },
            finish_reason: Some(output.finish_reason.into()),
        }],
        usage: Usage {
            prompt_tokens: output.prompt_tokens,
            completion_tokens,
            total_tokens,
        },
        mirmir: None,
    };

    Ok(Json(response))
}
