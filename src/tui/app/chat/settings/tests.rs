use super::*;

#[test]
fn applies_parameters_only_to_the_matching_model() {
    let mut app = App::new(true);
    app.chat_parameters = Some(ChatParameters {
        model: "selected".to_owned(),
        max_tokens: 64,
        temperature: 0.5,
        top_p: 0.8,
        top_k: 20,
        repetition_penalty: 1.1,
        seed: Some(42),
        token_limit: None,
    });
    let mut request = proto::GenerateRequest {
        model: "selected".to_owned(),
        ..Default::default()
    };
    app.apply_chat_parameters(&mut request);
    assert_eq!(request.max_tokens, Some(64));
    assert_eq!(request.seed, Some(42));
    request.model = "different".to_owned();
    request.max_tokens = None;
    app.apply_chat_parameters(&mut request);
    assert_eq!(request.max_tokens, None);
}

#[test]
fn arrows_step_generation_fields_within_the_token_limit() {
    let mut app = App::new(true);
    let mut dialog = ChatSettingsDialog::inspecting("model".to_owned());
    dialog.fields = ["8000", "1.95", "0.9", "40", "1.1", ""].map(str::to_owned);
    dialog.token_limit = Some(8_192);
    app.chat_settings_dialog = Some(dialog);
    app.adjust_chat_setting(1);
    app.adjust_chat_setting(1);
    app.move_chat_setting(true);
    app.adjust_chat_setting(1);
    app.adjust_chat_setting(1);
    for _ in 0..5 {
        app.move_chat_setting(true);
    }
    app.adjust_chat_setting(1);
    let dialog = app.chat_settings_dialog.expect("dialog");
    assert_eq!(dialog.fields[0], "8192");
    assert_eq!(dialog.fields[1], "2.00");
    assert_eq!(dialog.fields[5], "");
}
