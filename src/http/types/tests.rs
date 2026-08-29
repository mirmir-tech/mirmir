use super::*;

#[test]
fn exposes_reasoning_separately_from_final_content() {
    let response = CompletionResponse::new(
        "id".into(),
        1,
        "model".into(),
        result("answer", "draft", "", "stop"),
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

    let (_, request, image) = request.into_application().expect("valid vision request");

    assert!(image.is_some_and(|image| image.starts_with(b"\x89PNG")));
    assert_eq!(
        request.conversation.messages[0].content,
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
    let (_, request, _image) = request.into_application().expect("valid tool request");
    assert_eq!(request.conversation.tools[0].function.name, "weather");

    let response = CompletionResponse::new(
        "id".into(),
        1,
        "ministral".into(),
        result(
            "",
            "",
            r#"[{"id":"abc123456","type":"function","function":{"name":"weather","arguments":{"city":"Warsaw"}}}]"#,
            "tool_calls",
        ),
    );
    let value = serde_json::to_value(response).expect("serializable response");
    assert_eq!(value["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(value["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "weather");
}

#[test]
fn forwards_exact_generation_controls() {
    let request: ChatRequest = serde_json::from_value(serde_json::json!({
        "model": "benchmark-model",
        "messages": [{"role": "user", "content": "Continue"}],
        "max_tokens": 128,
        "min_tokens": 128,
        "ignore_eos": true
    }))
    .expect("OpenAI-compatible request");

    let (_, request, _image) = request.into_application().expect("valid generation request");
    assert_eq!(request.options.max_tokens, Some(128));
    assert_eq!(request.options.min_tokens, Some(128));
    assert_eq!(request.options.ignore_eos, Some(true));
}

fn result(
    text: &str,
    reasoning: &str,
    tool_calls: &str,
    finish_reason: &'static str,
) -> crate::application::GenerationResult {
    crate::application::GenerationResult {
        output: libmir::GenerationOutput {
            text: text.to_owned(),
            reasoning: reasoning.to_owned(),
            tool_calls: tool_calls.to_owned(),
            token_ids: Vec::new(),
            prompt_tokens: 0,
            finish_reason,
            metrics: libmir::GenerationMetrics::default(),
        },
        elapsed_ms: 0.0,
        ttft_ms: None,
        tokens_per_second: None,
    }
}
