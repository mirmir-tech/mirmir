use std::fs;

use serde::Serialize;

use super::{AppConfig, Store, schema::SecretsConfig};
use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct PresentedValue {
    pub key: &'static str,
    pub value: String,
    pub source: &'static str,
    pub editable: bool,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretPresentation {
    pub configured: bool,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigPresentation {
    pub values: Vec<PresentedValue>,
    pub hugging_face_token: SecretPresentation,
    pub http_api_key: SecretPresentation,
    pub config_path: String,
    pub secrets_path: String,
    pub raw_toml: String,
}

impl Store {
    pub(crate) fn configuration(&self) -> Result<ConfigPresentation> {
        let raw_toml = if self.paths.config_file.exists() {
            fs::read_to_string(&self.paths.config_file)?
        } else {
            toml::to_string_pretty(&AppConfig::default())?
        };
        let document: toml::Value = toml::from_str(&raw_toml)?;
        let config = self.load()?;
        let values = values(&config, &document);
        Ok(ConfigPresentation {
            values,
            hugging_face_token: self.hf_token_presentation()?,
            http_api_key: self.http_api_key_presentation()?,
            config_path: self.paths.config_file.display().to_string(),
            secrets_path: self.paths.secrets_file.display().to_string(),
            raw_toml,
        })
    }

    fn hf_token_presentation(&self) -> Result<SecretPresentation> {
        if std::env::var("HF_TOKEN").is_ok_and(|token| !token.is_empty()) {
            return Ok(SecretPresentation { configured: true, source: "environment" });
        }
        let secrets = self.load_secrets()?;
        Ok(secret(&secrets, |secrets| secrets.hugging_face.token.is_some()))
    }

    fn http_api_key_presentation(&self) -> Result<SecretPresentation> {
        if std::env::var("MIRMIR_HTTP_API_KEY").is_ok_and(|key| !key.is_empty()) {
            return Ok(SecretPresentation { configured: true, source: "environment" });
        }
        let secrets = self.load_secrets()?;
        Ok(secret(&secrets, |secrets| secrets.server.api_key.is_some()))
    }
}

fn secret(
    secrets: &SecretsConfig,
    configured: impl FnOnce(&SecretsConfig) -> bool,
) -> SecretPresentation {
    let configured = configured(secrets);
    SecretPresentation {
        configured,
        source: if configured {
            "secrets.toml"
        } else {
            "not configured"
        },
    }
}

fn values(config: &AppConfig, document: &toml::Value) -> Vec<PresentedValue> {
    let mut values = Vec::with_capacity(13);
    push(
        &mut values,
        document,
        "default_model",
        config.default_model.as_deref().unwrap_or("auto"),
        true,
        false,
    );
    server_values(&mut values, config, document);
    runtime_values(&mut values, config, document);
    values
}

fn server_values(values: &mut Vec<PresentedValue>, config: &AppConfig, document: &toml::Value) {
    push(values, document, "server.http_bind", &config.server.http_bind, true, true);
    push(
        values,
        document,
        "server.allow_remote",
        &config.server.allow_remote.to_string(),
        true,
        true,
    );
    push(
        values,
        document,
        "server.web_enabled",
        &config.server.web_enabled.to_string(),
        true,
        true,
    );
    push(
        values,
        document,
        "server.cors_origins",
        &config.server.cors_origins.join(", "),
        true,
        true,
    );
    push(
        values,
        document,
        "server.body_limit_bytes",
        &config.server.body_limit_bytes.to_string(),
        true,
        true,
    );
    push(
        values,
        document,
        "server.request_timeout_seconds",
        &config.server.request_timeout_seconds.to_string(),
        true,
        true,
    );
    push(
        values,
        document,
        "server.max_concurrency",
        &config.server.max_concurrency.to_string(),
        true,
        true,
    );
}

fn runtime_values(values: &mut Vec<PresentedValue>, config: &AppConfig, document: &toml::Value) {
    push(
        values,
        document,
        "runtime.kv_block_size",
        &optional(config.runtime.kv_block_size),
        true,
        true,
    );
    push(
        values,
        document,
        "runtime.kv_blocks",
        &optional(config.runtime.kv_blocks),
        true,
        true,
    );
    push(
        values,
        document,
        "runtime.kv_cache_dtype",
        &config
            .runtime
            .kv_cache_dtype
            .map_or_else(|| "auto".to_owned(), |value| format!("{value:?}")),
        false,
        true,
    );
    push(
        values,
        document,
        "runtime.max_batch_requests",
        &optional(config.runtime.max_batch_requests),
        true,
        true,
    );
    push(
        values,
        document,
        "runtime.max_batch_tokens",
        &optional(config.runtime.max_batch_tokens),
        true,
        true,
    );
}

fn push(
    values: &mut Vec<PresentedValue>,
    document: &toml::Value,
    key: &'static str,
    value: &str,
    editable: bool,
    restart_required: bool,
) {
    values.push(PresentedValue {
        key,
        value: value.to_owned(),
        source: if contains(document, key) {
            "config.toml"
        } else {
            "default"
        },
        editable,
        restart_required,
    });
}

fn contains(document: &toml::Value, key: &str) -> bool {
    key.split('.').try_fold(document, |value, part| value.get(part)).is_some()
}

fn optional(value: Option<impl ToString>) -> String {
    value.map_or_else(|| "auto".to_owned(), |value| value.to_string())
}
