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
