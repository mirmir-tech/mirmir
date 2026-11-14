use std::fs;

use serde::Serialize;

use super::{AppConfig, RemoteCertificate, ServerSettings, Store, schema::SecretsConfig};
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
    let mut values = Vec::with_capacity(16);
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
    let (certificate, key) = remote_tls_values(&config.server);
    for (key, value) in [
        ("server.allow_remote", config.server.allow_remote.to_string()),
        ("server.web_enabled", config.server.web_enabled.to_string()),
        ("server.cors_origins", config.server.cors_origins.join(", ")),
        ("server.body_limit_bytes", config.server.body_limit_bytes.to_string()),
        (
            "server.request_timeout_seconds",
            config.server.request_timeout_seconds.to_string(),
        ),
        ("server.max_concurrency", config.server.max_concurrency.to_string()),
    ] {
        push(values, document, key, &value, true, true);
    }
    // Changed only by `mirmir remote enable` and `disable`, with the listener.
    let remote = config.server.remote_bind.as_deref().unwrap_or("off");
    push(values, document, "server.remote_bind", remote, false, false);
    push(values, document, "server.remote_tls_cert", &certificate, true, true);
    push(values, document, "server.remote_tls_key", &key, true, true);
}

fn runtime_values(values: &mut Vec<PresentedValue>, config: &AppConfig, document: &toml::Value) {
    let runtime = &config.runtime;
    let dtype = runtime
        .kv_cache_dtype
        .map_or_else(|| "auto".to_owned(), |value| value.to_string());
    for (key, value) in [
        ("runtime.kv_block_size", optional(runtime.kv_block_size)),
        ("runtime.kv_blocks", optional(runtime.kv_blocks)),
        ("runtime.kv_cache_dtype", dtype),
        ("runtime.max_batch_requests", optional(runtime.max_batch_requests)),
        ("runtime.max_batch_tokens", optional(runtime.max_batch_tokens)),
        ("runtime.prefill_batch_wait_us", optional(runtime.prefill_batch_wait_us)),
        ("runtime.decode_batch_wait_us", optional(runtime.decode_batch_wait_us)),
        ("runtime.decode_priority_burst", optional(runtime.decode_priority_burst)),
        ("runtime.speculative_draft_tokens", optional(runtime.speculative_draft_tokens)),
        ("runtime.cached_prefill_policy", optional(runtime.cached_prefill_policy)),
        ("runtime.prefill_decode_policy", optional(runtime.prefill_decode_policy)),
        ("runtime.prefill_refill_policy", optional(runtime.prefill_refill_policy)),
        #[cfg(target_os = "macos")]
        ("runtime.metal_decode_reservation", optional(runtime.metal_decode_reservation)),
        ("runtime.memory_reserve_percent", optional(runtime.memory_reserve_percent)),
        ("runtime.memory_reserve_bytes", optional(runtime.memory_reserve_bytes)),
        ("runtime.vision_max_pixels", optional(runtime.vision_max_pixels)),
        (
            "runtime.vision_attention_budget_bytes",
            optional(runtime.vision_attention_budget_bytes),
        ),
        ("runtime.vision_memory_percent", optional(runtime.vision_memory_percent)),
    ] {
        push(values, document, key, &value, true, true);
    }
}

/// Both halves read `self-signed` when neither is set; the unset half of an
/// incomplete pair reads `missing`, since the listener refuses that pair.
fn remote_tls_values(server: &ServerSettings) -> (String, String) {
    let shown = |path: Option<&std::path::Path>| {
        path.map_or_else(|| "missing".to_owned(), |path| path.display().to_string())
    };
    match server.remote_certificate() {
        Ok(RemoteCertificate::SelfSigned) => ("self-signed".to_owned(), "self-signed".to_owned()),
        Ok(RemoteCertificate::Configured { .. }) | Err(_) => (
            shown(server.remote_tls_cert.as_deref()),
            shown(server.remote_tls_key.as_deref()),
        ),
    }
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
