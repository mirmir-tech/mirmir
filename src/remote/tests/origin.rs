//! A remote key reaches the same handlers as the local socket, but never the
//! server's security settings, its secrets or its file paths.

use tonic::{Code, Request};

use super::*;
use crate::rpc::proto::{
    runtime_server::Runtime, update_configuration_request::Operation as Change,
};

struct Remote {
    service: RuntimeService,
    root: std::path::PathBuf,
    client: rpc::Client,
}

async fn remote(scope: Scope) -> crate::error::Result<Remote> {
    let (service, root) = service();
    let status = service.remote().enable(Some("127.0.0.1:0"), service.clone()).await?;
    let listening = status.listening.expect("listener running");
    let (_, key) = service.remote().create_key("caller", scope)?;
    let address = listening.address.to_string();
    let client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;
    Ok(Remote { service, root, client })
}

impl Remote {
    async fn finish(self) -> crate::error::Result<()> {
        self.service.remote().shutdown().await;
        std::fs::remove_dir_all(self.root)?;
        Ok(())
    }
}

fn set(key: &str, value: &str) -> Change {
    Change::SetValue(proto::SetConfigurationValue {
        key: key.to_owned(),
        value: value.to_owned(),
    })
}

async fn change(client: &mut rpc::Client, operation: Change) -> Result<(), tonic::Status> {
    let request = proto::UpdateConfigurationRequest { operation: Some(operation) };
    client.update_configuration(request).await.map(drop)
}

#[tokio::test]
async fn a_control_key_cannot_change_server_settings_or_secrets() -> crate::error::Result<()> {
    let mut remote = remote(Scope::Control).await?;
    let before = remote.service.remote().status().await?.bind;
    for operation in [
        set("server.remote_bind", "0.0.0.0:7443"),
        set("server.remote_bind", "off"),
        set("server.remote_tls_cert", "/tmp/cert.pem"),
        set("server.remote_tls_key", "/tmp/key.pem"),
        set("server.http_bind", "0.0.0.0:8080"),
        set("server.allow_remote", "true"),
        Change::SetHfToken(proto::SetHfToken { token: "hf_remote".to_owned() }),
        Change::RemoveHfToken(proto::RemoveHfToken {}),
        Change::SetHttpApiKey(proto::SetHttpApiKey { key: "remote".to_owned() }),
        Change::RemoveHttpApiKey(proto::RemoveHttpApiKey {}),
    ] {
        let refused = change(&mut remote.client, operation.clone()).await.expect_err("refused");
        assert_eq!(refused.code(), Code::PermissionDenied, "{operation:?}: {refused}");
        assert!(refused.message().contains("local socket"), "{refused}");
    }
    let config = remote.service.remote().status().await?;
    assert_eq!(config.bind, before, "the listener setting is unchanged");
    let root = &remote.root;
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    assert!(!paths.secrets_file.exists(), "no secret was written");
    change(&mut remote.client, set("default_model", "chosen")).await?;
    remote.finish().await
}

#[tokio::test]
async fn remote_keys_see_no_raw_configuration_or_paths() -> crate::error::Result<()> {
    let mut remote = remote(Scope::Use).await?;
    let local = remote
        .service
        .get_configuration(Request::new(proto::GetConfigurationRequest {}))
        .await?
        .into_inner();
    assert!(!local.config_path.is_empty() && !local.secrets_path.is_empty());
    assert!(
        local
            .values
            .iter()
            .any(|value| value.key == "server.http_bind" && value.editable)
    );
    let seen = remote.client.get_configuration(proto::GetConfigurationRequest {}).await?;
    let seen = seen.into_inner();
    assert_eq!((seen.raw_toml.as_str(), seen.config_path.as_str()), ("", ""));
    assert_eq!(seen.secrets_path, "");
    assert_eq!(seen.values.len(), local.values.len());
    for value in &seen.values {
        let server = value.key.starts_with("server.");
        assert!(!(server && value.editable), "{} is local-only", value.key);
    }
    assert!(seen.values.iter().any(|value| value.key == "default_model" && value.editable));
    remote.finish().await
}
