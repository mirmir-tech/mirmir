use serde::Deserialize;

use super::{error::ApiError, types::ChatMessage};
use crate::{media::decode_data_url, rpc::proto};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
    Null,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Deserialize)]
pub(super) struct ImageUrl {
    url: String,
    #[serde(rename = "detail")]
    _detail: Option<String>,
}

pub(super) fn messages(
    messages: Vec<ChatMessage>,
) -> Result<(Vec<proto::ChatMessageInput>, Option<Vec<u8>>), ApiError> {
    let mut image = None;
    let messages = messages
        .into_iter()
        .map(|message| {
            let (content, message_image) = flatten_content(message.content)?;
            if let Some(message_image) = message_image
                && image.replace(message_image).is_some()
            {
                return Err(ApiError::bad_request("only one image per request is supported"));
            }
            Ok(proto::ChatMessageInput {
                role: message.role,
                content,
                reasoning_content: message.reasoning_content,
                tool_calls: message
                    .tool_calls
                    .unwrap_or_default()
                    .into_iter()
                    .map(proto_tool_call)
                    .collect(),
                tool_call_id: message.tool_call_id,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok((messages, image))
}

fn proto_tool_call(call: libmir::ChatToolCall) -> proto::ChatToolCall {
    proto::ChatToolCall {
        id: call.id,
        r#type: call.kind,
        function: Some(proto::ChatFunctionCall {
            name: call.function.name,
            arguments_json: call.function.arguments.to_string(),
        }),
    }
}

fn flatten_content(content: MessageContent) -> Result<(String, Option<Vec<u8>>), ApiError> {
    match content {
        MessageContent::Text(text) => Ok((text, None)),
        MessageContent::Null => Ok((String::new(), None)),
        MessageContent::Parts(parts) => {
            parts
                .into_iter()
                .try_fold((String::new(), None), |(mut text, mut image), part| {
                    match part {
                        ContentPart::Text { text: part } => text.push_str(&part),
                        ContentPart::ImageUrl { image_url } => {
                            let decoded = decode_data_url(&image_url.url)
                                .map_err(|error| ApiError::bad_request(error.to_string()))?;
                            if image.replace(decoded).is_some() {
                                return Err(ApiError::bad_request(
                                    "only one image per request is supported",
                                ));
                            }
                            text.push_str(libmir::IMAGE_PLACEHOLDER);
                        },
                    }
                    Ok((text, image))
                })
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_content_part_order_and_decodes_one_image() {
        let content: MessageContent = serde_json::from_value(serde_json::json!([
            {"type": "text", "text": "before"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="}},
            {"type": "text", "text": "after"}
        ]))
        .expect("valid parts");
        let (text, image) = flatten_content(content).expect("valid data URL");
        assert_eq!(text, format!("before{}after", libmir::IMAGE_PLACEHOLDER));
        assert!(image.is_some_and(|image| image.starts_with(b"\x89PNG")));
    }
}
