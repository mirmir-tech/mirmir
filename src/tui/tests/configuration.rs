use super::{
    super::app::{App, ConfigurationEdit, ConfigurationTarget, Screen},
    rendered,
};

#[test]
fn renders_configuration_without_exposing_secrets() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Settings;
    app.configuration = Some(crate::rpc::proto::ConfigurationSnapshot {
        values: vec![crate::rpc::proto::ConfigurationValue {
            key: "server.http_bind".to_owned(),
            value: "127.0.0.1:8080".to_owned(),
            source: "config.toml".to_owned(),
            editable: true,
            restart_required: true,
        }],
        hugging_face_token: Some(crate::rpc::proto::SecretState {
            configured: true,
            source: "secrets.toml".to_owned(),
        }),
        http_api_key: Some(crate::rpc::proto::SecretState {
            configured: false,
            source: "not configured".to_owned(),
        }),
        config_path: "/config/mirmir/config.toml".to_owned(),
        secrets_path: "/config/mirmir/secrets.toml".to_owned(),
        raw_toml: "schema_version = 1".to_owned(),
    });
    app.configuration_edit = Some(ConfigurationEdit {
        target: ConfigurationTarget::HuggingFaceToken,
        input: "hf_secret".to_owned(),
    });
    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("SETTINGS"));
    assert!(text.contains("hugging_face.token"));
    assert!(text.contains("configured"));
    assert!(text.contains("•••••••••"));
    assert!(!text.contains("hf_secret"));
    Ok(())
}
