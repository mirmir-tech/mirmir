use std::{convert::Infallible, time::Duration};

use axum::response::{
    IntoResponse, Response,
    sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;

use super::{error::ApiError, types::Usage};
use crate::rpc::proto;

pub fn response(
    mut source: ReceiverStream<Result<proto::GenerateEvent, tonic::Status>>,
    id: String,
    created: u64,
    model: String,
    include_usage: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Response {
    let (sender, receiver) = mpsc::channel(32);
    drop(tokio::spawn(async move {
        if !send_json(
            &sender,
            chunk(&id, created, &model, &json!({"role": "assistant"}), None, None),
        )
        .await
        {
            return;
        }
        loop {
            let event = tokio::select! {
                _result = shutdown.changed() => return,
                event = source.next() => event,
            };
            let Some(event) = event else {
                break;
            };
            match event {
                Ok(event) => {
                    if !handle_event(&sender, event, &id, created, &model, include_usage).await {
                        return;
                    }
                },
                Err(status) => {
                    let error = ApiError::status_envelope(status);
                    let _sent = send_json(&sender, error).await;
                    break;
                },
            }
        }
        let _sent = sender.send(Ok(Event::default().data("[DONE]"))).await;
    }));
    Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"))
        .into_response()
}

async fn handle_event(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    event: proto::GenerateEvent,
    id: &str,
    created: u64,
    model: &str,
    include_usage: bool,
) -> bool {
    match event.event {
        Some(proto::generate_event::Event::Token(token)) => {
            let Some(delta) = token_delta(&token) else {
                return true;
            };
            send_json(sender, chunk(id, created, model, &delta, None, None)).await
        },
        Some(proto::generate_event::Event::Completion(completion)) => {
            if !completion.tool_calls.is_empty() {
                let calls = completion
                    .tool_calls
                    .iter()
                    .enumerate()
                    .filter_map(tool_call_delta)
                    .collect::<Vec<_>>();
                if !send_json(
                    sender,
                    chunk(id, created, model, &json!({"tool_calls": calls}), None, None),
                )
                .await
                {
                    return false;
                }
            }
            let usage = include_usage.then(|| Usage::from_completion(&completion));
            send_json(
                sender,
                chunk(
                    id,
                    created,
                    model,
                    &json!({}),
                    Some(completion.finish_reason.as_str()),
                    usage.as_ref(),
                ),
            )
            .await
        },
        Some(proto::generate_event::Event::Started(_)) | None => true,
    }
}

fn token_delta(token: &proto::Token) -> Option<serde_json::Value> {
    if token.channel == "tool_calls" {
        None
    } else if token.reasoning || token.channel == "reasoning" {
        Some(json!({"reasoning_content": token.text}))
    } else {
        Some(json!({"content": token.text}))
    }
}

fn tool_call_delta((index, call): (usize, &proto::ChatToolCall)) -> Option<serde_json::Value> {
    let function = call.function.as_ref()?;
    Some(json!({
        "index": index,
        "id": call.id,
        "type": call.r#type,
        "function": {
            "name": function.name,
            "arguments": function.arguments_json,
        }
    }))
}

fn chunk(
    id: &str,
    created: u64,
    model: &str,
    delta: &serde_json::Value,
    finish_reason: Option<&str>,
    usage: Option<&Usage>,
) -> serde_json::Value {
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}],
        "usage": usage,
    })
}

async fn send_json(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    value: impl serde::Serialize,
) -> bool {
    let Ok(data) = serde_json::to_string(&value) else {
        return false;
    };
    sender.send(Ok(Event::default().data(data))).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_reasoning_to_a_separate_openai_delta() {
        let delta = token_delta(&proto::Token {
            id: 1,
            text: "draft".into(),
            reasoning: true,
            channel: "reasoning".into(),
        });
        assert_eq!(delta, Some(json!({"reasoning_content": "draft"})));
    }

    #[test]
    fn suppresses_native_tool_json_tokens() {
        let delta = token_delta(&proto::Token {
            id: 9,
            text: r#"[{"name":"weather"}]"#.into(),
            reasoning: false,
            channel: "tool_calls".into(),
        });
        assert_eq!(delta, None);
    }
}
