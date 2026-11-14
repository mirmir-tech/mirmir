use tonic::Status;

use super::RuntimeService;
use crate::{
    application::ConfigurationChange,
    config::{self, ConfigPresentation, SecretPresentation},
    remote::{self, Origin},
    rpc::proto,
};

pub(super) fn snapshot(
    service: &RuntimeService,
    origin: &Origin,
) -> Result<proto::ConfigurationSnapshot, Status> {
    let configuration = service.application.configuration().map_err(super::status::application)?;
    Ok(presented(convert(configuration), origin))
}

/// What a caller sees of the configuration. Remote keys get neither the raw
/// file nor the server's file paths, and server settings read as not
/// editable since only the local socket may change them.
fn presented(
    mut snapshot: proto::ConfigurationSnapshot,
    origin: &Origin,
) -> proto::ConfigurationSnapshot {
    if origin.is_remote() {
        snapshot.raw_toml.clear();
        snapshot.config_path.clear();
        snapshot.secrets_path.clear();
        for value in &mut snapshot.values {
            let change = ConfigurationChange::SetValue {
                key: value.key.clone(),
                value: String::new(),
            };
            value.editable &= remote::reserved(&change).is_none();
        }
    }
    snapshot
}

pub(super) async fn update(
    service: &RuntimeService,
    origin: &Origin,
    request: proto::UpdateConfigurationRequest,
) -> Result<proto::UpdateConfigurationResponse, Status> {
    use proto::update_configuration_request::Operation;

    let change = match request.operation {
        Some(Operation::SetValue(update)) => {
            ConfigurationChange::SetValue { key: update.key, value: update.value }
        },
        Some(Operation::SetHfToken(update)) => ConfigurationChange::SetHfToken(update.token),
        Some(Operation::RemoveHfToken(_)) => ConfigurationChange::RemoveHfToken,
        Some(Operation::TestHfToken(_)) => ConfigurationChange::TestHfToken,
        Some(Operation::SetHttpApiKey(update)) => ConfigurationChange::SetHttpApiKey(update.key),
        Some(Operation::RemoveHttpApiKey(_)) => ConfigurationChange::RemoveHttpApiKey,
        None => return Err(Status::invalid_argument("configuration operation is required")),
    };
    if let Origin::Remote(caller) = origin
        && let Some(reserved) = remote::reserved(&change)
    {
        let what = reserved.description();
        tracing::warn!(key = %caller.key_name, what, "refused a remote configuration change");
        return Err(Status::permission_denied(format!(
            "{what} can be changed only over the server's local socket"
        )));
    }
    if origin.is_remote()
        && let ConfigurationChange::SetValue { key, value } = &change
        && key.trim() == "default_model"
    {
        // The default model answers remote requests that name no model, so a
        // remote key may choose only a model it could also name directly.
        let application = service.application.clone();
        let selector = value.trim().to_owned();
        if !config::automatic(&selector) {
            super::status::internal(
                tokio::task::spawn_blocking(move || application.check_offered(&selector)).await,
            )?
            .map_err(super::status::application)?;
        }
    }
    let outcome = service
        .application
        .update_configuration(change)
        .await
        .map_err(super::status::application)?;
    Ok(proto::UpdateConfigurationResponse {
        configuration: Some(presented(convert(outcome.configuration), origin)),
        message: outcome.message,
        restart_required: outcome.restart_required,
    })
}

fn convert(config: ConfigPresentation) -> proto::ConfigurationSnapshot {
    proto::ConfigurationSnapshot {
        values: config
            .values
            .into_iter()
            .map(|value| proto::ConfigurationValue {
                key: value.key.to_owned(),
                value: value.value,
                source: value.source.to_owned(),
                editable: value.editable,
                restart_required: value.restart_required,
            })
            .collect(),
        hugging_face_token: Some(secret(&config.hugging_face_token)),
        http_api_key: Some(secret(&config.http_api_key)),
        config_path: config.config_path,
        secrets_path: config.secrets_path,
        raw_toml: config.raw_toml,
    }
}

fn secret(secret: &SecretPresentation) -> proto::SecretState {
    proto::SecretState {
        configured: secret.configured,
        source: secret.source.to_owned(),
    }
}
