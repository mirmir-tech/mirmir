use super::*;

#[test]
fn maps_reasoning_to_a_separate_openai_delta() {
    let delta = token_delta(&libmir::GenerationToken {
        preceding_ids: Vec::new(),
        id: 1,
        text: "draft".into(),
        channel: libmir::GenerationChannel::Reasoning,
    });
    assert_eq!(delta, Some(json!({"reasoning_content": "draft"})));
}

#[test]
fn suppresses_native_tool_json_tokens() {
    let delta = token_delta(&libmir::GenerationToken {
        preceding_ids: Vec::new(),
        id: 9,
        text: r#"[{"name":"weather"}]"#.into(),
        channel: libmir::GenerationChannel::ToolCalls,
    });
    assert_eq!(delta, None);
}

#[tokio::test]
async fn output_start_sends_one_role_chunk() {
    let (sender, mut receiver) = mpsc::channel(1);
    let mut role_sent = false;
    let mut emitted_token_ids = 0;
    assert!(
        handle_event(
            &sender,
            GenerationEvent::OutputStarted,
            ChunkContext {
                id: "id",
                created: 0,
                model: "model",
                include_usage: false,
                return_token_ids: false,
            },
            &mut role_sent,
            &mut emitted_token_ids,
        )
        .await
    );
    assert!(role_sent);
    assert!(receiver.try_recv().is_ok());
    assert!(receiver.try_recv().is_err());
}

#[test]
fn attaches_the_role_only_to_the_first_delta() {
    let mut sent = false;
    assert_eq!(with_role(json!({}), &mut sent), json!({"role": "assistant"}));
    assert_eq!(with_role(json!({"content": "first"}), &mut sent), json!({"content": "first"}));
    assert_eq!(with_role(json!({"content": "second"}), &mut sent), json!({"content": "second"}));
}

#[test]
fn includes_requested_token_ids_on_the_choice() {
    let value = chunk("id", 0, "model", &json!({"content": "x"}), None, None, Some(&[42]));
    assert_eq!(value["choices"][0]["token_ids"], json!([42]));
}
