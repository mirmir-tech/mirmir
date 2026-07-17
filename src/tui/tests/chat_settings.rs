use super::rendered;
use crate::tui::app::{App, ChatParameters, ChatSettingsDialog, ChatSettingsStatus, Screen};

#[test]
fn renders_chat_parameter_editor_and_active_indicator() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Chat;
    app.models.push(crate::rpc::proto::ModelInfo {
        id: "Qwen--Test".to_owned(),
        path: "/models/qwen".to_owned(),
    });
    app.chat_parameters = Some(parameters());
    app.chat_settings_dialog = Some(ChatSettingsDialog {
        model: "Qwen--Test".to_owned(),
        fields: ["256", "0.7", "0.9", "40", "1.1", ""].map(str::to_owned),
        selected: 0,
        save_default: true,
        persisted: true,
        status: ChatSettingsStatus::Editing,
        error: None,
    });

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("CHAT PARAMETERS"));
    assert!(text.contains("Mirmir model defaults"));
    assert!(text.contains("repetition penalty"));
    assert!(text.contains("seed"));
    assert!(text.contains("auto"));
    assert!(text.contains("Save as model default"));
    assert!(text.contains("ONE-OFF PARAMETERS"));
    Ok(())
}

fn parameters() -> ChatParameters {
    ChatParameters {
        model: "Qwen--Test".to_owned(),
        max_tokens: 256,
        temperature: 0.7,
        top_p: 0.9,
        top_k: 40,
        repetition_penalty: 1.1,
        seed: None,
    }
}
