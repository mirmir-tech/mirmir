use super::*;

#[test]
fn exposes_reasoning_separately_from_final_content() {
    let response = CompletionResponse::new(
        "id".into(),
        1,
        "model".into(),
        proto::Completion {
            text: "answer".into(),
            reasoning: "draft".into(),
            ..Default::default()
        },
    );
    let value = serde_json::to_value(response).expect("serializable response");
    assert_eq!(value["choices"][0]["message"]["content"], "answer");
    assert_eq!(value["choices"][0]["message"]["reasoning_content"], "draft");
}

#[test]
fn accepts_openai_image_url_content_parts() {
    let request: ChatRequest = serde_json::from_value(serde_json::json!({
        "model": "vision-model",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="}},
                {"type": "text", "text": "What is shown?"}
            ]
        }]
    }))
    .expect("OpenAI-compatible request");

    let request = request.into_proto().expect("valid vision request");

    assert!(request.image.is_some_and(|image| image.starts_with(b"\x89PNG")));
    assert_eq!(
        request.messages[0].content,
        format!("{}What is shown?", libmir::IMAGE_PLACEHOLDER)
    );
}

#[test]
fn maps_tools_and_tool_calls_through_openai_protocol() {
    let request: ChatRequest = serde_json::from_value(serde_json::json!({
        "model": "ministral",
        "messages": [{"role": "user", "content": "Weather in Warsaw?"}],
        "tools": [{"type": "function", "function": {
            "name": "weather",
            "parameters": {"type": "object"}
        }}]
    }))
    .expect("OpenAI-compatible tool request");
    let proto = request.into_proto().expect("valid tool request");
    assert_eq!(
        proto.tools[0].function.as_ref().map(|function| function.name.as_str()),
        Some("weather")
    );

    let response = CompletionResponse::new(
        "id".into(),
        1,
        "ministral".into(),
        proto::Completion {
            finish_reason: "tool_calls".into(),
            tool_calls: vec![proto::ChatToolCall {
                id: "abc123456".into(),
                r#type: "function".into(),
                function: Some(proto::ChatFunctionCall {
                    name: "weather".into(),
                    arguments_json: r#"{"city":"Warsaw"}"#.into(),
                }),
            }],
            ..Default::default()
        },
    );
    let value = serde_json::to_value(response).expect("serializable response");
    assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(value["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "weather");
}
