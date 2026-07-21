use axum::{
    Json,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tonic::Request;

use super::{security_headers, session::WebError};
use crate::{
    http::ApiState,
    rpc::{proto, proto::runtime_server::Runtime},
};

#[derive(Serialize)]
pub struct Configuration {
    values: Vec<Setting>,
    config_path: String,
    secrets_path: String,
    raw_toml: String,
}

#[derive(Serialize)]
struct Setting {
    key: String,
    value: String,
    source: String,
    restart_required: bool,
    kind: SettingKind,
    actions: Vec<SettingAction>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SettingKind {
    Value,
    Secret,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SettingAction {
    Edit,
    Test,
    Remove,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum UpdateRequest {
    #[serde(rename = "set_value")]
    Set {
        key: String,
        #[serde(rename = "value")]
        new_value: String,
    },
    #[serde(rename = "remove_value")]
    Remove { key: String },
    #[serde(rename = "test_value")]
    Test { key: String },
}

#[derive(Serialize)]
struct Updated {
    configuration: Configuration,
    message: String,
    restart_required: bool,
}

pub async fn configuration(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let configuration = state
        .service()
        .get_configuration(Request::new(proto::GetConfigurationRequest {}))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
    Ok((security_headers(), Json(Configuration::from(configuration))).into_response())
}

pub async fn update_configuration(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(update): Json<UpdateRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let response = state
        .service()
        .update_configuration(Request::new(proto::UpdateConfigurationRequest {
            operation: Some(operation(update).map_err(WebError::from_status)?),
        }))
        .await
        .map_err(WebError::from_status)?
        .into_inner();
    let configuration = response
        .configuration
        .ok_or_else(|| WebError::runtime("runtime omitted updated configuration"))?;
    state.configuration_changed();
    Ok((
        security_headers(),
        Json(Updated {
            configuration: Configuration::from(configuration),
            message: response.message,
            restart_required: response.restart_required,
        }),
    )
        .into_response())
}

fn operation(
    request: UpdateRequest,
) -> Result<proto::update_configuration_request::Operation, tonic::Status> {
    use proto::update_configuration_request::Operation;
    match request {
        UpdateRequest::Set { key, new_value } => match key.as_str() {
            "hugging_face.token" => {
                Ok(Operation::SetHfToken(proto::SetHfToken { token: new_value }))
            },
            "server.api_key" => {
                Ok(Operation::SetHttpApiKey(proto::SetHttpApiKey { key: new_value }))
            },
            _ => Ok(Operation::SetValue(proto::SetConfigurationValue { key, value: new_value })),
        },
        UpdateRequest::Remove { key } => match key.as_str() {
            "hugging_face.token" => Ok(Operation::RemoveHfToken(proto::RemoveHfToken {})),
            "server.api_key" => Ok(Operation::RemoveHttpApiKey(proto::RemoveHttpApiKey {})),
            _ => Err(tonic::Status::invalid_argument(format!("cannot remove `{key}`"))),
        },
        UpdateRequest::Test { key } if key == "hugging_face.token" => {
            Ok(Operation::TestHfToken(proto::TestHfToken {}))
        },
        UpdateRequest::Test { key } => {
            Err(tonic::Status::invalid_argument(format!("cannot test `{key}`")))
        },
    }
}

impl From<proto::ConfigurationSnapshot> for Configuration {
    fn from(config: proto::ConfigurationSnapshot) -> Self {
        let mut values: Vec<_> = config.values.into_iter().map(Setting::from).collect();
        values.push(Setting::secret(
            "hugging_face.token",
            config.hugging_face_token.unwrap_or_default(),
            false,
            true,
        ));
        values.push(Setting::secret(
            "server.api_key",
            config.http_api_key.unwrap_or_default(),
            true,
            false,
        ));
        Self {
            values,
            config_path: config.config_path,
            secrets_path: config.secrets_path,
            raw_toml: config.raw_toml,
        }
    }
}

impl From<proto::ConfigurationValue> for Setting {
    fn from(value: proto::ConfigurationValue) -> Self {
        Self {
            key: value.key,
            value: value.value,
            source: value.source,
            restart_required: value.restart_required,
            kind: SettingKind::Value,
            actions: if value.editable {
                vec![SettingAction::Edit]
            } else {
                Vec::new()
            },
        }
    }
}

impl Setting {
    fn secret(
        key: &str,
        secret: proto::SecretState,
        restart_required: bool,
        testable: bool,
    ) -> Self {
        let mut actions = vec![SettingAction::Edit];
        if secret.configured && testable {
            actions.push(SettingAction::Test);
        }
        if secret.configured {
            actions.push(SettingAction::Remove);
        }
        Self {
            key: key.to_owned(),
            value: if secret.configured {
                "********".to_owned()
            } else {
                "—".to_owned()
            },
            source: secret.source,
            restart_required,
            kind: SettingKind::Secret,
            actions,
        }
    }
}
