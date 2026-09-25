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
    for field in ["reasoning_budget", "reasoning_effort", "thinking_token_budget"] {
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
fn accepts_vllm_thinking_alias_and_rejects_conflicts_or_unknown_options() {
    for (enabled, expected) in
        [(true, libmir::ReasoningMode::Enabled), (false, libmir::ReasoningMode::Disabled)]
    {
        let body = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "chat_template_kwargs":{"enable_thinking":enabled}});
        let request: ChatRequest = serde_json::from_value(body).expect("wire alias");
        assert_eq!(request.into_application().expect("compatible alias").1.reasoning, expected);
    }
    for explicit in ["enabled", "model_default"] {
        let body = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "reasoning":explicit, "chat_template_kwargs":{"enable_thinking":false}});
        let request: ChatRequest = serde_json::from_value(body).expect("wire conflict");
        assert!(request.into_application().is_err());
    }
    let body = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "chat_template_kwargs":{"thinking_budget":32}});
    assert!(serde_json::from_value::<ChatRequest>(body).is_err());
}

#[test]
fn reasoning_does_not_change_conflicting_budget_validation() {
    let request: ChatRequest = serde_json::from_value(serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}], "reasoning":"disabled", "max_tokens":64, "max_completion_tokens":128})).expect("wire input");
    assert!(request.into_application().is_err());
}
