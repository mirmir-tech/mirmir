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
use tonic::Request;

use super::{security_headers, session::WebError};
use crate::{
    http::ApiState,
    media::decode_data_url,
    rpc::{proto, proto::runtime_server::Runtime},
};

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
pub struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: Option<u64>,
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
        return Err(WebError::from_status(tonic::Status::invalid_argument(
            "model and at least one message are required",
        )));
    }
    let request = proto::GenerateRequest::try_from(chat)?;
    let mut source = state
        .service()
        .generate(Request::new(request))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
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
                    let terminal =
                        matches!(&event.event, Some(proto::generate_event::Event::Completion(_)));
                    (stream_event(event), terminal)
                },
                Err(status) => (error_event(&status), true),
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

fn stream_event(event: proto::GenerateEvent) -> Event {
    match event.event {
        Some(proto::generate_event::Event::Started(started)) => {
            json_event("started", Started { operation_id: started.operation_id })
        },
        Some(proto::generate_event::Event::Token(token)) => json_event(
            "token",
            Token {
                text: token.text,
                reasoning: token.reasoning,
            },
        ),
        Some(proto::generate_event::Event::Completion(completion)) => {
            json_event("completion", CompletionPayload::from(completion))
        },
        None => Event::default().event("error").data("{\"message\":\"empty event\"}"),
    }
}

fn error_event(status: &tonic::Status) -> Event {
    json_event(
        "error",
        StreamError {
            message: status.message().to_owned(),
            code: format!("{:?}", status.code()),
        },
    )
}

fn json_event(name: &'static str, value: impl Serialize) -> Event {
    serde_json::to_string(&value).map_or_else(
        |_| Event::default().event("error").data("{\"message\":\"serialization failed\"}"),
        |data| Event::default().event(name).data(data),
    )
}

impl TryFrom<ChatRequest> for proto::GenerateRequest {
    type Error = WebError;

    fn try_from(chat: ChatRequest) -> Result<Self, Self::Error> {
        let mut messages: Vec<proto::ChatMessageInput> =
            chat.messages.into_iter().map(Into::into).collect();
        let image = chat
            .image
            .map(|value| {
                decode_data_url(&value).map_err(|error| {
                    WebError::from_status(tonic::Status::invalid_argument(error.to_string()))
                })
            })
            .transpose()?;
        if image.is_some() {
            let message =
                messages.iter_mut().rev().find(|message| message.role == "user").ok_or_else(
                    || {
                        WebError::from_status(tonic::Status::invalid_argument(
                            "an image requires a user message",
                        ))
                    },
                )?;
            message.content = format!("{}\n{}", libmir::IMAGE_PLACEHOLDER, message.content);
        }
        Ok(Self {
            model: chat.model,
            prompt: String::new(),
            max_tokens: chat.max_tokens,
            temperature: chat.temperature,
            top_p: chat.top_p,
            top_k: chat.top_k,
            repetition_penalty: chat.repetition_penalty,
            seed: chat.seed,
            messages,
            image,
            tools: Vec::new(),
            tool_choice_json: None,
        })
    }
}

impl From<Message> for proto::ChatMessageInput {
    fn from(message: Message) -> Self {
        Self {
            role: message.role,
            content: message.content,
            reasoning_content: message.reasoning_content,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

impl From<proto::Completion> for CompletionPayload {
    fn from(completion: proto::Completion) -> Self {
        Self {
            text: completion.text,
            reasoning: completion.reasoning,
            prompt_tokens: completion.prompt_tokens,
            completion_tokens: completion.completion_tokens,
            finish_reason: completion.finish_reason,
            elapsed_ms: completion.elapsed_ms,
            tokens_per_second: completion.tokens_per_second,
            ttft_ms: completion.ttft_ms,
            prefill_tokens_per_second: completion.prefill_tokens_per_second,
            decode_tokens_per_second: completion.decode_tokens_per_second,
            prefill_ms: completion.prefill_ms,
            decode_ms: completion.decode_ms,
        }
    }
}
