use serde_json::json;

use super::super::types::Usage;

pub(super) fn with_role(mut delta: serde_json::Value, role_sent: &mut bool) -> serde_json::Value {
    if !*role_sent {
        if let Some(fields) = delta.as_object_mut() {
            fields.insert("role".to_owned(), json!("assistant"));
        }
        *role_sent = true;
    }
    delta
}

pub(super) fn token_delta(token: &libmir::GenerationToken) -> Option<serde_json::Value> {
    if token.channel == libmir::GenerationChannel::ToolCalls {
        None
    } else if token.channel == libmir::GenerationChannel::Reasoning {
        Some(json!({"reasoning_content": token.text}))
    } else {
        Some(json!({"content": token.text}))
    }
}

pub(super) fn tool_call_delta((index, call): (usize, &libmir::ToolCall)) -> serde_json::Value {
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

pub(super) fn chunk(
    id: &str,
    created: u64,
    model: &str,
    delta: &serde_json::Value,
    finish_reason: Option<&str>,
    usage: Option<&Usage>,
    token_ids: Option<&[u32]>,
) -> serde_json::Value {
    let mut choice = json!({"index": 0, "delta": delta, "finish_reason": finish_reason});
    if let Some(token_ids) = token_ids {
        choice["token_ids"] = json!(token_ids);
    }
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [choice],
        "usage": usage,
    })
}
