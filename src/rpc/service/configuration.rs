use tonic::Status;

use super::RuntimeService;
use crate::{
    config::{ConfigPresentation, SecretPresentation},
    rpc::proto,
};

pub(super) fn snapshot(service: &RuntimeService) -> Result<proto::ConfigurationSnapshot, Status> {
    service.store.configuration().map(convert).map_err(internal)
}

pub(super) async fn update(
    service: &RuntimeService,
    request: proto::UpdateConfigurationRequest,
) -> Result<proto::UpdateConfigurationResponse, Status> {
    use proto::update_configuration_request::Operation;

    let (message, restart_required) = match request.operation {
        Some(Operation::SetValue(update)) => {
            service.store.set_config_value(&update.key, &update.value).map_err(invalid)?;
            let restart = update.key != "default_model";
            tracing::info!(key = %update.key, restart_required = restart, "configuration updated");
            (format!("saved {}", update.key), restart)
        },
        Some(Operation::SetHfToken(update)) => {
            service.store.set_hf_token(update.token.trim()).map_err(invalid)?;
            tracing::info!("Hugging Face token updated");
            ("Hugging Face token saved".to_owned(), false)
        },
        Some(Operation::RemoveHfToken(_)) => {
            service.store.remove_hf_token().map_err(internal)?;
            tracing::info!("Hugging Face token removed from secrets.toml");
            ("stored Hugging Face token removed".to_owned(), false)
        },
        Some(Operation::TestHfToken(_)) => {
            let identity = service.catalog.test_hf_token().await.map_err(unavailable)?;
            tracing::info!(identity = %identity, "Hugging Face token verified");
            (format!("token valid for {identity}"), false)
        },
        Some(Operation::SetHttpApiKey(update)) => {
            service.store.set_http_api_key(update.key.trim()).map_err(invalid)?;
            tracing::info!("HTTP API key updated");
            ("HTTP API key saved".to_owned(), true)
        },
        Some(Operation::RemoveHttpApiKey(_)) => {
            service.store.remove_http_api_key().map_err(internal)?;
            tracing::info!("HTTP API key removed from secrets.toml");
            ("stored HTTP API key removed".to_owned(), true)
        },
        None => return Err(Status::invalid_argument("configuration operation is required")),
    };
    Ok(proto::UpdateConfigurationResponse {
        configuration: Some(snapshot(service)?),
        message,
        restart_required,
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

fn internal(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

fn invalid(error: impl std::fmt::Display) -> Status {
    Status::invalid_argument(error.to_string())
}

fn unavailable(error: impl std::fmt::Display) -> Status {
    Status::unavailable(error.to_string())
}
