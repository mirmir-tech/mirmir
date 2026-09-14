use super::*;

#[test]
fn forwards_reasoning_mode_and_a_total_completion_budget() {
    for (wire, expected) in [
        ("model_default", libmir::ReasoningMode::ModelDefault),
        ("enabled", libmir::ReasoningMode::Enabled),
        ("disabled", libmir::ReasoningMode::Disabled),
    ] {
        let request: ChatRequest = serde_json::from_value(serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "reasoning":wire, "max_completion_tokens":1024})).expect("valid wire request");
        let (_, request, _) = request.into_application().expect("valid application request");
        assert_eq!(request.reasoning, expected);
        assert_eq!(request.options.max_tokens, Some(1024));
    }
}

#[test]
fn omission_keeps_the_existing_model_default() {
    let request: ChatRequest = serde_json::from_value(
        serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}]}),
    )
    .expect("request");
    let (_, request, _) = request.into_application().expect("request");
    assert_eq!(request.reasoning, libmir::ReasoningMode::ModelDefault);
}

#[test]
fn rejects_unsupported_reasoning_budgets_effort_and_invalid_modes() {
    for field in ["reasoning_budget", "reasoning_effort"] {
        let mut body =
            serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}]});
        body[field] = serde_json::json!(32);
        let request: ChatRequest = serde_json::from_value(body).expect("wire input");
        assert!(request.into_application().is_err(), "unsupported {field} was silently ignored");
    }
    let request = serde_json::from_value::<ChatRequest>(
        serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "reasoning":"low"}),
    );
    assert!(request.is_err());
}

#[test]
fn reasoning_does_not_change_conflicting_budget_validation() {
    let request: ChatRequest = serde_json::from_value(serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "reasoning":"disabled", "max_tokens":64, "max_completion_tokens":128})).expect("wire input");
    assert!(request.into_application().is_err());
}
