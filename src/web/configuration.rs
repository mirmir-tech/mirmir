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
    hugging_face_token: Secret,
    http_api_key: Secret,
    config_path: String,
    secrets_path: String,
    raw_toml: String,
}

#[derive(Serialize)]
struct Setting {
    key: String,
    value: String,
    source: String,
    editable: bool,
    restart_required: bool,
}

#[derive(Serialize)]
struct Secret {
    configured: bool,
    source: String,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum UpdateRequest {
    SetValue {
        key: String,
        #[serde(rename = "value")]
        new_value: String,
    },
    SetHfToken {
        token: String,
    },
    RemoveHfToken,
    TestHfToken,
    SetHttpApiKey {
        key: String,
    },
    RemoveHttpApiKey,
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
            operation: Some(update.into()),
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

impl From<UpdateRequest> for proto::update_configuration_request::Operation {
    fn from(request: UpdateRequest) -> Self {
        match request {
            UpdateRequest::SetValue { key, new_value } => {
                Self::SetValue(proto::SetConfigurationValue { key, value: new_value })
            },
            UpdateRequest::SetHfToken { token } => Self::SetHfToken(proto::SetHfToken { token }),
            UpdateRequest::RemoveHfToken => Self::RemoveHfToken(proto::RemoveHfToken {}),
            UpdateRequest::TestHfToken => Self::TestHfToken(proto::TestHfToken {}),
            UpdateRequest::SetHttpApiKey { key } => {
                Self::SetHttpApiKey(proto::SetHttpApiKey { key })
            },
            UpdateRequest::RemoveHttpApiKey => Self::RemoveHttpApiKey(proto::RemoveHttpApiKey {}),
        }
    }
}

impl From<proto::ConfigurationSnapshot> for Configuration {
    fn from(config: proto::ConfigurationSnapshot) -> Self {
        Self {
            values: config.values.into_iter().map(Setting::from).collect(),
            hugging_face_token: Secret::from(config.hugging_face_token.unwrap_or_default()),
            http_api_key: Secret::from(config.http_api_key.unwrap_or_default()),
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
            editable: value.editable,
            restart_required: value.restart_required,
        }
    }
}

impl From<proto::SecretState> for Secret {
    fn from(secret: proto::SecretState) -> Self {
        Self {
            configured: secret.configured,
            source: secret.source,
        }
    }
}
