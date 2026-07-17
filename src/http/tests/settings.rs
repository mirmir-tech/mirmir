#[test]
fn remote_bind_requires_explicit_opt_in() {
    let mut settings = crate::config::ServerSettings {
        http_bind: "0.0.0.0:8080".to_owned(),
        ..crate::config::ServerSettings::default()
    };
    assert!(settings.validate().is_err());
    settings.allow_remote = true;
    assert!(settings.validate().is_ok());
    settings.web_enabled = true;
    assert!(settings.validate().is_err());
}
