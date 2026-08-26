use tonic::Status;

use super::RuntimeService;
use crate::{
    application::ConfigurationChange,
    config::{ConfigPresentation, SecretPresentation},
    rpc::proto,
};

pub(super) fn snapshot(service: &RuntimeService) -> Result<proto::ConfigurationSnapshot, Status> {
    Ok(convert(super::status::internal(service.coordinator.configuration())?))
}

pub(super) async fn update(
    service: &RuntimeService,
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
    let outcome = service
        .coordinator
        .update_configuration(change)
        .await
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    Ok(proto::UpdateConfigurationResponse {
        configuration: Some(convert(outcome.configuration)),
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
