use futures_util::StreamExt;
use js_sys::Uint8Array;
use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use wasm_streams::ReadableStream;

use super::{ChatRequest, ChatState, Parameters, UiMessage};
use crate::{api, state::RuntimeState, types::ChatMessage};

#[derive(Deserialize)]
struct Token {
    text: String,
    reasoning: bool,
}

#[derive(Deserialize)]
struct Started {
    operation_id: String,
}

#[derive(Deserialize)]
struct Completion {
    text: String,
    reasoning: String,
    #[serde(rename = "completion_tokens")]
    output_tokens: u64,
    finish_reason: String,
    tokens_per_second: Option<f64>,
    ttft_ms: Option<f64>,
    prefill_tokens_per_second: Option<f64>,
    decode_tokens_per_second: Option<f64>,
}

pub fn run_generation(
    runtime: RuntimeState,
    chat: ChatState,
    model: String,
    prompt: String,
    parameters: Parameters,
) {
    if model.is_empty() {
        runtime.notify("Select a loaded model", true);
        return;
    }
    let image = chat.attachment.get_untracked().map(|item| item.data_url);
    chat.attachment.set(None);
    chat.messages.update(|messages| {
        messages.push(UiMessage {
            role: "user".to_owned(),
            content: prompt,
            ..UiMessage::default()
        });
        messages.push(UiMessage {
            role: "assistant".to_owned(),
            thinking: true,
            ..UiMessage::default()
        });
    });
    let messages = chat
        .messages
        .get_untracked()
        .into_iter()
        .filter(|message| message.role != "assistant" || !message.content.is_empty())
        .map(|message| ChatMessage {
            role: message.role,
            content: message.content,
            reasoning_content: (!message.reasoning.is_empty()).then_some(message.reasoning),
        })
        .collect();
    let parameters = parameters.request();
    let request = ChatRequest {
        model,
        messages,
        max_tokens: parameters.max_tokens,
        temperature: parameters.temperature,
        top_p: parameters.top_p,
        top_k: parameters.top_k,
        repetition_penalty: parameters.repetition_penalty,
        seed: parameters.seed,
        image,
    };
    chat.running.set(true);
    chat.status.set("starting".to_owned());
    spawn_local(async move {
        let result = async {
            let response = api::raw_post(runtime, "/chat", &request).await?;
            let body = response.body().ok_or_else(|| "chat response has no stream".to_owned())?;
            let mut source = ReadableStream::from_raw(body).into_stream();
            let mut buffer = Vec::new();
            while let Some(chunk) = source.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => return Err(format!("stream error: {error:?}")),
                };
                let bytes = Uint8Array::new(&chunk);
                let mut current = vec![0; bytes.length() as usize];
                bytes.copy_to(&mut current);
                buffer.extend(current);
                consume_events(&mut buffer, |event, data| apply_event(runtime, chat, event, data))?;
            }
            consume_events(&mut buffer, |event, data| apply_event(runtime, chat, event, data))
        }
        .await;
        if let Err(error) = result {
            runtime.notify(error.clone(), true);
            chat.status.set("failed".to_owned());
            update_assistant(chat, |message| {
                message.thinking = false;
                message.content = format!("**Error:** {error}");
            });
        }
        chat.running.set(false);
        chat.operation_id.set(None);
    });
}

fn consume_events<F>(buffer: &mut Vec<u8>, mut apply: F) -> Result<(), String>
where
    F: FnMut(&str, &str) -> Result<(), String>,
{
    while let Some(end) = buffer.windows(2).position(|window| window == b"\n\n") {
        let event = crate::result::string(String::from_utf8(buffer.drain(..end + 2).collect()))?;
        let mut kind = "message";
        let mut data = String::new();
        for line in event.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                kind = value.trim();
            }
            if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.trim());
            }
        }
        apply(kind, &data)?;
    }
    Ok(())
}

fn apply_event(
    runtime: RuntimeState,
    chat: ChatState,
    event: &str,
    data: &str,
) -> Result<(), String> {
    match event {
        "started" => {
            let value: Started = crate::result::string(serde_json::from_str(data))?;
            chat.operation_id.set(Some(value.operation_id));
            chat.status.set("running".to_owned());
        },
        "token" => {
            let token: Token = crate::result::string(serde_json::from_str(data))?;
            update_assistant(chat, |message| {
                message.thinking = token.reasoning;
                if token.reasoning {
                    message.reasoning.push_str(&token.text);
                } else {
                    message.content.push_str(&token.text);
                }
            });
        },
        "completion" => {
            let value: Completion = crate::result::string(serde_json::from_str(data))?;
            update_assistant(chat, |message| {
                message.thinking = false;
                message.content = value.text;
                message.reasoning = value.reasoning;
            });
            chat.ttft.set(value.ttft_ms);
            chat.prefill.set(value.prefill_tokens_per_second);
            chat.decode.set(value.decode_tokens_per_second);
            chat.throughput.set(value.tokens_per_second);
            chat.status.set(value.finish_reason.clone());
            if value.finish_reason == "max_tokens" {
                runtime.notify(format!("Response stopped at the {}-token limit. Increase Max tokens in Parameters or model settings.", value.output_tokens), true);
            }
        },
        "error" => {
            let value: serde_json::Value = crate::result::string(serde_json::from_str(data))?;
            return Err(value["message"].as_str().unwrap_or("generation failed").to_owned());
        },
        _ => {},
    }
    Ok(())
}

fn update_assistant(chat: ChatState, update: impl FnOnce(&mut UiMessage)) {
    chat.messages.update(|messages| {
        if let Some(message) = messages.last_mut() {
            update(message);
        }
    });
}
