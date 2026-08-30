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
use crate::application::GenerationEvent;

pub fn response(
    mut source: ReceiverStream<crate::application::Result<GenerationEvent>>,
    id: String,
    created: u64,
    model: String,
    include_usage: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Response {
    let (sender, receiver) = mpsc::channel(32);
    drop(tokio::spawn(async move {
        let mut role_sent = false;
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
                    if !handle_event(
                        &sender, event, &id, created, &model, include_usage, &mut role_sent,
                    )
                    .await
                    {
                        return;
                    }
                },
                Err(error) => {
                    if !role_sent
                        && !send_json(
                            &sender,
                            chunk(&id, created, &model, &json!({"role": "assistant"}), None, None),
                        )
                        .await
                    {
                        return;
                    }
                    let error = ApiError::application_envelope(&error);
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
    event: GenerationEvent,
    id: &str,
    created: u64,
    model: &str,
    include_usage: bool,
    role_sent: &mut bool,
) -> bool {
    match event {
        GenerationEvent::Token(token) => {
            let Some(delta) = token_delta(&token) else {
                return true;
            };
            let delta = with_role(delta, role_sent);
            send_json(sender, chunk(id, created, model, &delta, None, None)).await
        },
        GenerationEvent::Completion(completion) => {
            let calls =
                libmir::ToolCall::parse_mistral(&completion.output.tool_calls).unwrap_or_default();
            if !calls.is_empty() {
                let calls = calls.iter().enumerate().map(tool_call_delta).collect::<Vec<_>>();
                let delta = with_role(json!({"tool_calls": calls}), role_sent);
                if !send_json(sender, chunk(id, created, model, &delta, None, None)).await {
                    return false;
                }
            }
            let usage = include_usage.then(|| Usage::from_result(&completion));
            let delta = with_role(json!({}), role_sent);
            send_json(
                sender,
                chunk(
                    id,
                    created,
                    model,
                    &delta,
                    Some(completion.output.finish_reason),
                    usage.as_ref(),
                ),
            )
            .await
        },
        GenerationEvent::Started { .. } => true,
    }
}

fn with_role(mut delta: serde_json::Value, role_sent: &mut bool) -> serde_json::Value {
    if !*role_sent {
        if let Some(fields) = delta.as_object_mut() {
            fields.insert("role".to_owned(), json!("assistant"));
        }
        *role_sent = true;
    }
    delta
}

fn token_delta(token: &libmir::GenerationToken) -> Option<serde_json::Value> {
    if token.channel == libmir::GenerationChannel::ToolCalls {
        None
    } else if token.channel == libmir::GenerationChannel::Reasoning {
        Some(json!({"reasoning_content": token.text}))
    } else {
        Some(json!({"content": token.text}))
    }
}

fn tool_call_delta((index, call): (usize, &libmir::ToolCall)) -> serde_json::Value {
    json!({
        "index": index,
        "id": call.id,
        "type": call.kind,
        "function": {
            "name": call.function.name,
            "arguments": call.function.arguments.to_string(),
        }
    })
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
        let delta = token_delta(&libmir::GenerationToken {
            id: 1,
            text: "draft".into(),
            channel: libmir::GenerationChannel::Reasoning,
        });
        assert_eq!(delta, Some(json!({"reasoning_content": "draft"})));
    }

    #[test]
    fn suppresses_native_tool_json_tokens() {
        let delta = token_delta(&libmir::GenerationToken {
            id: 9,
            text: r#"[{"name":"weather"}]"#.into(),
            channel: libmir::GenerationChannel::ToolCalls,
        });
        assert_eq!(delta, None);
    }

    #[test]
    fn attaches_the_role_only_to_the_first_delta() {
        let mut sent = false;
        assert_eq!(
            with_role(json!({"content": "first"}), &mut sent),
            json!({"role": "assistant", "content": "first"})
        );
        assert_eq!(
            with_role(json!({"content": "second"}), &mut sent),
            json!({"content": "second"})
        );
    }
}
