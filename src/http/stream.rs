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

mod payload;
#[cfg(test)]
mod tests;

use payload::{chunk, token_delta, tool_call_delta, with_role};

#[derive(Clone, Copy)]
struct ChunkContext<'a> {
    id: &'a str,
    created: u64,
    model: &'a str,
    include_usage: bool,
    return_token_ids: bool,
}

pub fn response(
    mut source: ReceiverStream<crate::application::Result<GenerationEvent>>,
    id: String,
    created: u64,
    model: String,
    include_usage: bool,
    return_token_ids: bool,
    mut shutdown: watch::Receiver<bool>,
) -> Response {
    let (sender, receiver) = mpsc::channel(32);
    drop(tokio::spawn(async move {
        let mut role_sent = false;
        let mut emitted_token_ids = 0;
        let context = ChunkContext {
            id: &id,
            created,
            model: &model,
            include_usage,
            return_token_ids,
        };
        loop {
            let event = tokio::select! {
                _result = shutdown.changed() => return,
                () = sender.closed() => return,
                event = source.next() => event,
            };
            let Some(event) = event else {
                break;
            };
            match event {
                Ok(event) => {
                    if !handle_event(
                        &sender,
                        event,
                        context,
                        &mut role_sent,
                        &mut emitted_token_ids,
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
                            chunk(
                                &id,
                                created,
                                &model,
                                &json!({"role": "assistant"}),
                                None,
                                None,
                                None,
                            ),
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
    context: ChunkContext<'_>,
    role_sent: &mut bool,
    emitted_token_ids: &mut usize,
) -> bool {
    match event {
        GenerationEvent::OutputStarted => {
            if *role_sent {
                true
            } else {
                let delta = with_role(json!({}), role_sent);
                send_json(
                    sender,
                    chunk(context.id, context.created, context.model, &delta, None, None, None),
                )
                .await
            }
        },
        GenerationEvent::Token(token) => {
            let delta = token_delta(&token);
            if delta.is_none() && !context.return_token_ids {
                return true;
            }
            let delta = with_role(delta.unwrap_or_else(|| json!({})), role_sent);
            let token_ids = context.return_token_ids.then(|| {
                let mut ids = token.preceding_ids;
                ids.push(token.id);
                ids
            });
            let sent = send_json(
                sender,
                chunk(
                    context.id,
                    context.created,
                    context.model,
                    &delta,
                    None,
                    None,
                    token_ids.as_deref(),
                ),
            )
            .await;
            if sent {
                *emitted_token_ids += token_ids.as_ref().map_or(0, Vec::len);
            }
            sent
        },
        GenerationEvent::Completion(completion) => {
            let calls =
                libmir::ToolCall::parse_mistral(&completion.output.tool_calls).unwrap_or_default();
            if !calls.is_empty() {
                let calls = calls.iter().enumerate().map(tool_call_delta).collect::<Vec<_>>();
                let delta = with_role(json!({"tool_calls": calls}), role_sent);
                if !send_json(
                    sender,
                    chunk(context.id, context.created, context.model, &delta, None, None, None),
                )
                .await
                {
                    return false;
                }
            }
            let usage = context.include_usage.then(|| Usage::from_result(&completion));
            let trailing_ids = context.return_token_ids.then(|| {
                completion
                    .output
                    .token_ids
                    .get((*emitted_token_ids).min(completion.output.token_ids.len())..)
                    .unwrap_or_default()
            });
            let delta = with_role(json!({}), role_sent);
            send_json(
                sender,
                chunk(
                    context.id,
                    context.created,
                    context.model,
                    &delta,
                    Some(completion.output.finish_reason),
                    usage.as_ref(),
                    trailing_ids,
                ),
            )
            .await
        },
        GenerationEvent::Started { .. } => true,
    }
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
