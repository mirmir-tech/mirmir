use super::*;

#[test]
fn tool_constraints_are_explicit_and_typed() -> Result<(), Box<dyn std::error::Error>> {
    let base = serde_json::json!({"model":"qwen","messages":[{"role":"user","content":"Hi"}]});
    let request: ChatRequest = serde_json::from_value(base.clone())?;
    let (_, request, _) = request.into_application().map_err(|_| "unexpected request error")?;
    assert_eq!(request.tool_constraints, libmir::ToolConstraints::None);
    let mut body = base;
    body["tool_constraints"] = serde_json::json!("schema");
    let request: ChatRequest = serde_json::from_value(body.clone())?;
    let (_, request, _) = request.into_application().map_err(|_| "unexpected request error")?;
    assert_eq!(request.tool_constraints, libmir::ToolConstraints::Schema);
    body["tool_constraints"] = serde_json::json!("repair_afterwards");
    assert!(serde_json::from_value::<ChatRequest>(body).is_err());
    Ok(())
}

#[test]
fn response_format_json_schema_constrains_content() -> Result<(), Box<dyn std::error::Error>> {
    let schema = serde_json::json!({"type":"object","properties":{"facts":{"type":"array"}}});
    let mut body = serde_json::json!({"model":"gemma","messages":[{"role":"user","content":"Hi"}],
        "max_tokens":400,"chat_template_kwargs":{"enable_thinking":true},
        "response_format":{"type":"json_schema","json_schema":{"name":"submit_facts","schema":schema}}});
    let request: ChatRequest = serde_json::from_value(body.clone())?;
    let (_, request, _) = request.into_application().map_err(|_| "unexpected request error")?;
    assert_eq!(request.response_format, libmir::ResponseFormat::JsonSchema(schema));
    assert_eq!(request.reasoning_token_budget.map(std::num::NonZeroUsize::get), Some(300));

    body["response_format"] = serde_json::json!({"type":"json_object"});
    let request: ChatRequest = serde_json::from_value(body.clone())?;
    let (_, request, _) = request.into_application().map_err(|_| "unexpected request error")?;
    assert_eq!(
        request.response_format,
        libmir::ResponseFormat::JsonSchema(serde_json::json!({"type":"object"}))
    );

    body["tool_constraints"] = serde_json::json!("schema");
    let request: ChatRequest = serde_json::from_value(body.clone())?;
    assert!(request.into_application().is_err());
    body["tool_constraints"] = serde_json::json!("none");
    body["response_format"] = serde_json::json!({"type":"json_schema",
        "json_schema":{"name":"x","schema":{"type":"object"},"strict":false}});
    let request: ChatRequest = serde_json::from_value(body)?;
    assert!(request.into_application().is_err());
    Ok(())
}
