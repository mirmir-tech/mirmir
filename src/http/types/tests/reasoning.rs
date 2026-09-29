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

#[test]
fn schema_reasoning_budget_is_typed_bounded_and_explicit() {
    let body = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}],
        "chat_template_kwargs":{"enable_thinking":true}, "tool_constraints":"schema",
        "max_tokens":4096,"thinking_token_budget":3072});
    let request: ChatRequest = serde_json::from_value(body.clone()).expect("request");
    let (_, request, _) = request.into_application().expect("valid budget");
    assert_eq!(request.reasoning_token_budget.map(std::num::NonZeroUsize::get), Some(3072));
    assert_eq!(request.options.max_tokens, Some(4096));
    for value in [
        serde_json::json!(0),
        serde_json::json!(4095),
        serde_json::json!(4096),
        serde_json::json!(u64::MAX),
    ] {
        let mut invalid = body.clone();
        invalid["thinking_token_budget"] = value;
        let request: ChatRequest = serde_json::from_value(invalid).expect("wire request");
        assert!(request.into_application().is_err());
    }
    for (field, value) in [
        ("max_tokens", serde_json::Value::Null),
        ("tool_constraints", serde_json::json!("none")),
        ("chat_template_kwargs", serde_json::json!({"enable_thinking":false})),
    ] {
        let mut invalid = body.clone();
        invalid[field] = value;
        let request: ChatRequest = serde_json::from_value(invalid).expect("wire request");
        assert!(request.into_application().is_err());
    }
    for value in [
        serde_json::json!(-1),
        serde_json::json!(1.5),
        serde_json::json!("32"),
        serde_json::json!(true),
    ] {
        let mut invalid = body.clone();
        invalid["thinking_token_budget"] = value;
        assert!(serde_json::from_value::<ChatRequest>(invalid).is_err());
    }
}

#[test]
fn schema_reasoning_reserves_output_when_client_omits_allowance() {
    for (total, expected) in [(4096_usize, 3072), (512, 384), (3, 1)] {
        let body = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}],
            "chat_template_kwargs":{"enable_thinking":true}, "tool_constraints":"schema",
            "max_completion_tokens":total});
        let (_, request, _) = serde_json::from_value::<ChatRequest>(body)
            .expect("wire input")
            .into_application()
            .expect("bounded schema reasoning");
        assert_eq!(request.reasoning_token_budget.map(std::num::NonZeroUsize::get), Some(expected));
        assert_eq!(request.options.max_tokens, Some(total));
    }
}

#[test]
fn implicit_output_reservation_does_not_change_other_modes_or_override_explicit_budget() {
    let base = serde_json::json!({"model":"qwen", "messages":[{"role":"user","content":"Hi"}],
        "reasoning":"enabled", "tool_constraints":"schema", "max_tokens":4096});
    for (field, value, expected) in [
        ("reasoning", serde_json::json!("disabled"), None),
        ("reasoning", serde_json::json!("model_default"), None),
        ("tool_constraints", serde_json::json!("none"), None),
        ("max_tokens", serde_json::Value::Null, None),
        ("thinking_token_budget", serde_json::json!(1024), Some(1024)),
    ] {
        let mut body = base.clone();
        body[field] = value;
        let (_, request, _) = serde_json::from_value::<ChatRequest>(body)
            .expect("wire input")
            .into_application()
            .expect("valid request");
        assert_eq!(request.reasoning_token_budget.map(std::num::NonZeroUsize::get), expected);
    }
}
