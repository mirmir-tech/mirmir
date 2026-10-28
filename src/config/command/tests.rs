use super::*;

#[tokio::test]
async fn explicit_server_never_falls_back_to_local_configuration() {
    let instance = Instance::Socket(
        std::env::temp_dir().join(format!("mirmir-missing-{}.sock", std::process::id())),
    );
    for command in [
        ConfigCommand::Show,
        ConfigCommand::Validate,
        ConfigCommand::Set {
            key: "server.max_concurrency".to_owned(),
            value: Some("7".to_owned()),
        },
        ConfigCommand::Remove { key: HTTP_API_KEY.to_owned() },
        ConfigCommand::Test { key: HF_TOKEN.to_owned() },
    ] {
        assert!(matches!(run(&instance, command).await, Err(Error::Attach { .. })));
    }
}

#[tokio::test]
async fn attached_mode_rejects_local_file_operations() {
    for command in [ConfigCommand::Init, ConfigCommand::Edit] {
        let error = run(&Instance::System, command).await.expect_err("local command rejected");
        assert!(matches!(error, Error::Config(_)));
        assert!(error.to_string().contains("local operations"));
    }
}
