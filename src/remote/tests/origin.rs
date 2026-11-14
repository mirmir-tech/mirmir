//! A remote key reaches the same handlers as the local socket, but never the
//! server's security settings, its secrets, its file paths or directories it
//! does not offer as models.

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
    let listening = status.listening().cloned().expect("listener running");
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
    let before = remote.service.remote().status().await?.bind().map(str::to_owned);
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
    assert_eq!(config.bind(), before.as_deref(), "the listener setting is unchanged");
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

/// The status a remote `InspectModel` ends with for `selector`.
async fn inspect(client: &mut rpc::Client, selector: &str) -> tonic::Status {
    let request = proto::InspectModelRequest { selector: selector.to_owned() };
    client.inspect_model(request).await.expect_err("no model is inspectable here")
}

#[tokio::test]
async fn remote_keys_name_only_models_the_server_offers() -> crate::error::Result<()> {
    let mut remote = remote(Scope::Control).await?;
    let directory = remote.root.join("state");
    let missing = remote.root.join("nothing-here");
    let present = inspect(&mut remote.client, &directory.display().to_string()).await;
    let absent = inspect(&mut remote.client, &missing.display().to_string()).await;
    assert_eq!(present.code(), Code::InvalidArgument, "{present}");
    assert_eq!(absent.code(), present.code());
    let anonymous = |status: &tonic::Status, path: &std::path::Path| {
        status.message().replace(&path.display().to_string(), "<path>")
    };
    assert_eq!(anonymous(&present, &directory), anonymous(&absent, &missing));

    let generate = proto::GenerateRequest {
        model: directory.display().to_string(),
        prompt: "hello".to_owned(),
        ..Default::default()
    };
    let refused = remote.client.generate(generate).await.expect_err("an unlisted path");
    assert_eq!(refused.code(), Code::InvalidArgument, "{refused}");
    let embed = proto::EmbedRequest {
        model: "/".to_owned(),
        inputs: vec!["text".to_owned()],
        ..Default::default()
    };
    let refused = remote.client.embed(embed).await.expect_err("the root is no model");
    assert_eq!(refused.message(), "model request is invalid: /", "{refused}");

    let snapshot = remote.root.join("state").join("hf-hub/models--Owner--Model/snapshots/commit");
    std::fs::create_dir_all(&snapshot)?;
    let listed = remote.client.list_local_models(proto::ListLocalModelsRequest {}).await?;
    let selector = listed
        .into_inner()
        .models
        .into_iter()
        .find(|model| model.repo_id == "Owner/Model");
    let selector = selector.expect("the cached snapshot is listed").selector;
    let offered = remote
        .client
        .inspect_model(proto::InspectModelRequest { selector: selector.clone() })
        .await;
    if let Err(status) = offered {
        assert_ne!(status.message(), format!("model request is invalid: {selector}"));
    }
    remote.finish().await
}
