use axum::{
    Json,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use super::{security_headers, session::WebError};
use crate::{
    application::ConfigurationChange,
    config::{ConfigPresentation, PresentedValue, SecretPresentation},
    http::ApiState,
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
    let configuration = state.application().configuration().map_err(WebError::application)?;
    Ok((security_headers(), Json(Configuration::from(configuration))).into_response())
}

pub async fn update_configuration(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(update): Json<UpdateRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let operation = operation(update)?;
    let response = state
        .application()
        .update_configuration(operation)
        .await
        .map_err(WebError::application)?;
    state.configuration_changed();
    Ok((
        security_headers(),
        Json(Updated {
            configuration: Configuration::from(response.configuration),
            message: response.message,
            restart_required: response.restart_required,
        }),
    )
        .into_response())
}

fn operation(request: UpdateRequest) -> Result<ConfigurationChange, WebError> {
    match request {
        UpdateRequest::Set { key, new_value } => match key.as_str() {
            "hugging_face.token" => Ok(ConfigurationChange::SetHfToken(new_value)),
            "server.api_key" => Ok(ConfigurationChange::SetHttpApiKey(new_value)),
            _ => Ok(ConfigurationChange::SetValue { key, value: new_value }),
        },
        UpdateRequest::Remove { key } => match key.as_str() {
            "hugging_face.token" => Ok(ConfigurationChange::RemoveHfToken),
            "server.api_key" => Ok(ConfigurationChange::RemoveHttpApiKey),
            _ => Err(WebError::invalid(format!("cannot remove `{key}`"))),
        },
        UpdateRequest::Test { key } if key == "hugging_face.token" => {
            Ok(ConfigurationChange::TestHfToken)
        },
        UpdateRequest::Test { key } => Err(WebError::invalid(format!("cannot test `{key}`"))),
    }
}

impl From<ConfigPresentation> for Configuration {
    fn from(config: ConfigPresentation) -> Self {
        let mut values: Vec<_> = config.values.into_iter().map(Setting::from).collect();
        values.push(Setting::secret("hugging_face.token", &config.hugging_face_token, false, true));
        values.push(Setting::secret("server.api_key", &config.http_api_key, true, false));
        Self {
            values,
            config_path: config.config_path,
            secrets_path: config.secrets_path,
            raw_toml: config.raw_toml,
        }
    }
}

impl From<PresentedValue> for Setting {
    fn from(value: PresentedValue) -> Self {
        Self {
            key: value.key.to_owned(),
            value: value.value,
            source: value.source.to_owned(),
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
        secret: &SecretPresentation,
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
            source: secret.source.to_owned(),
            restart_required,
            kind: SettingKind::Secret,
            actions,
        }
    }
}
