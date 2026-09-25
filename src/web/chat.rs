use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::State,
    http::HeaderMap,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{security_headers, session::WebError};
use crate::{
    application::{GenerationEvent, GenerationResult},
    http::ApiState,
    media::decode_data_url,
};

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
pub struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: Option<u64>,
    min_tokens: Option<u64>,
    ignore_eos: Option<bool>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<u64>,
    repetition_penalty: Option<f32>,
    seed: Option<u64>,
    image: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    role: String,
    content: String,
    reasoning_content: Option<String>,
}

#[derive(Serialize)]
struct Started {
    operation_id: String,
}

#[derive(Serialize)]
struct Token {
    text: String,
    reasoning: bool,
}

#[derive(Serialize)]
struct CompletionPayload {
    text: String,
    reasoning: String,
    prompt_tokens: u64,
    completion_tokens: u64,
    finish_reason: String,
    elapsed_ms: f64,
    tokens_per_second: Option<f64>,
    ttft_ms: Option<f64>,
    prefill_tokens_per_second: Option<f64>,
    decode_tokens_per_second: Option<f64>,
    prefill_ms: Option<f64>,
    decode_ms: Option<f64>,
}

#[derive(Serialize)]
struct StreamError {
    message: String,
    code: String,
}

pub async fn chat(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(chat): Json<ChatRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    if chat.model.trim().is_empty() || chat.messages.is_empty() {
        return Err(WebError::invalid("model and at least one message are required"));
    }
    let (selector, request, image) = application_request(chat)?;
    let mut source = state.application().generation_stream(&selector, request, image);
    let mut shutdown = state.shutdown();
    let (sender, receiver) = mpsc::channel::<Result<Event, Infallible>>(32);
    drop(tokio::spawn(async move {
        loop {
            let event = tokio::select! {
                _result = shutdown.changed() => break,
                event = source.next() => event,
            };
            let Some(event) = event else {
                break;
            };
            let (event, terminal) = match event {
                Ok(event) => {
                    let terminal = matches!(&event, GenerationEvent::Completion(_));
                    (stream_event(event), terminal)
                },
                Err(error) => (error_event(&error), true),
            };
            if sender.send(Ok(event)).await.is_err() || terminal {
                break;
            }
        }
    }));
    let mut response = Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"))
        .into_response();
    response.headers_mut().extend(security_headers());
    Ok(response)
}

fn stream_event(event: GenerationEvent) -> Event {
    match event {
        GenerationEvent::Started { operation_id } => {
            json_event("started", Started { operation_id })
        },
        GenerationEvent::OutputStarted => Event::default().event("output_started").data("{}"),
        GenerationEvent::Token(token) => json_event(
            "token",
            Token {
                text: token.text,
                reasoning: token.channel == libmir::GenerationChannel::Reasoning,
            },
        ),
        GenerationEvent::Completion(completion) => {
            json_event("completion", CompletionPayload::from(*completion))
        },
    }
}

fn error_event(error: &crate::application::Error) -> Event {
    json_event(
        "error",
        StreamError {
            message: error.to_string(),
            code: "application_error".to_owned(),
        },
    )
}

fn json_event(name: &'static str, value: impl Serialize) -> Event {
    serde_json::to_string(&value).map_or_else(
        |_| Event::default().event("error").data("{\"message\":\"serialization failed\"}"),
        |data| Event::default().event(name).data(data),
    )
}

fn application_request(
    chat: ChatRequest,
) -> Result<(String, libmir::GenerationRequest, Option<Vec<u8>>), WebError> {
    let mut messages: Vec<libmir::Message> = chat.messages.into_iter().map(Into::into).collect();
    let image = match chat.image {
        Some(value) => match decode_data_url(&value) {
            Ok(image) => Some(image),
            Err(error) => {
                return Err(WebError::invalid(error.to_string()));
            },
        },
        None => None,
    };
    if image.is_some() {
        let message = messages
            .iter_mut()
            .rev()
            .find(|message| message.role == "user")
            .ok_or_else(|| WebError::invalid("an image requires a user message"))?;
        message.content = format!("{}\n{}", libmir::IMAGE_PLACEHOLDER, message.content);
    }
    let selector = chat.model.clone();
    let request = libmir::GenerationRequest {
        tool_constraints: libmir::ToolConstraints::None,
        conversation: libmir::Conversation {
            messages,
            tools: Vec::new(),
            tool_choice: libmir::ToolChoice::Auto,
        },
        options: libmir::GenerationOverrides {
            max_tokens: optional_usize(chat.max_tokens, "max_tokens")?,
            min_tokens: optional_usize(chat.min_tokens, "min_tokens")?,
            ignore_eos: chat.ignore_eos,
            temperature: chat.temperature,
            top_p: chat.top_p,
            top_k: optional_usize(chat.top_k, "top_k")?,
            repetition_penalty: chat.repetition_penalty,
        },
        seed: chat.seed,
        reasoning_cycle: libmir::ReasoningCyclePolicy::default(),
        reasoning: libmir::ReasoningMode::ModelDefault,
    };
    Ok((selector, request, image))
}

fn optional_usize(value: Option<u64>, field: &str) -> Result<Option<usize>, WebError> {
    value
        .map(|value| {
            usize::try_from(value).map_err(|_| WebError::invalid(format!("{field} is too large")))
        })
        .transpose()
}

impl From<Message> for libmir::Message {
    fn from(message: Message) -> Self {
        Self {
            role: message.role,
            content: message.content,
            reasoning_content: message.reasoning_content,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

impl From<GenerationResult> for CompletionPayload {
    fn from(completion: GenerationResult) -> Self {
        let output = completion.output;
        Self {
            text: output.text,
            reasoning: output.reasoning,
            prompt_tokens: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
            completion_tokens: u64::try_from(output.token_ids.len()).unwrap_or(u64::MAX),
            finish_reason: output.finish_reason.to_owned(),
            elapsed_ms: completion.elapsed_ms,
            tokens_per_second: completion.tokens_per_second,
            ttft_ms: completion.ttft_ms,
            prefill_tokens_per_second: output.metrics.throughput.prefill.per_second,
            decode_tokens_per_second: output.metrics.throughput.decode.per_second,
            prefill_ms: Some(output.metrics.durations_ms.prefill),
            decode_ms: Some(output.metrics.durations_ms.decode),
        }
    }
}
