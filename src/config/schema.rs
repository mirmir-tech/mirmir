use std::{net::SocketAddr, path::PathBuf, time::Duration};

use libmir::KvCacheDType;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub schema_version: u32,
    pub default_model: Option<String>,
    pub runtime: RuntimeSettings,
    pub server: ServerSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSettings {
    pub http_bind: String,
    pub allow_remote: bool,
    pub web_enabled: bool,
    pub cors_origins: Vec<String>,
    pub body_limit_bytes: usize,
    pub request_timeout_seconds: u64,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeSettings {
    pub kv_block_size: Option<usize>,
    pub kv_blocks: Option<u32>,
    pub kv_cache_dtype: Option<KvCacheDType>,
    pub max_batch_requests: Option<usize>,
    pub max_batch_tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub schema_version: u32,
    pub id: String,
    pub path: PathBuf,
    #[serde(default)]
    pub hub: Option<HubModelConfig>,
    #[serde(default)]
    pub generation: GenerationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HubModelConfig {
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GenerationConfig {
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<usize>,
    pub repetition_penalty: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct SecretsConfig {
    pub schema_version: u32,
    pub hugging_face: HuggingFaceSecrets,
    pub server: ServerSecrets,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ServerSecrets {
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct HuggingFaceSecrets {
    pub token: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            default_model: None,
            runtime: RuntimeSettings::default(),
            server: ServerSettings::default(),
        }
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            hugging_face: HuggingFaceSecrets::default(),
            server: ServerSecrets::default(),
        }
    }
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            http_bind: "127.0.0.1:8080".to_owned(),
            allow_remote: false,
            web_enabled: false,
            cors_origins: Vec::new(),
            body_limit_bytes: 1_048_576,
            request_timeout_seconds: 300,
            max_concurrency: 16,
        }
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<()> {
        validate_schema(self.schema_version)?;
        if matches!(self.runtime.kv_block_size, Some(0)) {
            return Err(Error::Config("runtime.kv_block_size must be positive".to_owned()));
        }
        if matches!(self.runtime.kv_blocks, Some(0)) {
            return Err(Error::Config("runtime.kv_blocks must be positive".to_owned()));
        }
        self.server.validate()?;
        Ok(())
    }
}

impl ServerSettings {
    pub fn address(&self) -> Result<SocketAddr> {
        self.http_bind.parse().map_err(|error| {
            Error::Config(format!("server.http_bind `{}` is invalid: {error}", self.http_bind))
        })
    }

    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds)
    }

    pub fn validate(&self) -> Result<()> {
        let address = self.address()?;
        if !address.ip().is_loopback() && !self.allow_remote {
            return Err(Error::Config(
                "server.allow_remote must be true for a non-loopback HTTP bind".to_owned(),
            ));
        }
        if self.web_enabled && !address.ip().is_loopback() {
            return Err(Error::Config(
                "server.web_enabled currently requires a loopback HTTP bind".to_owned(),
            ));
        }
        if self.body_limit_bytes == 0 || self.request_timeout_seconds == 0 {
            return Err(Error::Config("HTTP body limit and timeout must be positive".to_owned()));
        }
        if self.max_concurrency == 0 {
            return Err(Error::Config("server.max_concurrency must be positive".to_owned()));
        }
        Ok(())
    }
}

impl ModelConfig {
    pub fn validate(&self, expected_id: &str) -> Result<()> {
        validate_schema(self.schema_version)?;
        if self.id != expected_id {
            return Err(Error::Config(format!(
                "model id `{}` does not match file id `{expected_id}`",
                self.id
            )));
        }
        if self.path.as_os_str().is_empty() {
            return Err(Error::Config(format!("model `{expected_id}` has an empty path")));
        }
        Ok(())
    }
}

impl GenerationConfig {
    #[must_use]
    pub const fn has_overrides(self) -> bool {
        self.max_tokens.is_some()
            || self.temperature.is_some()
            || self.top_p.is_some()
            || self.top_k.is_some()
            || self.repetition_penalty.is_some()
    }
}

impl RuntimeSettings {
    #[must_use]
    pub fn to_libmir(&self) -> libmir::RuntimeConfig {
        let mut config = libmir::RuntimeConfig::default();
        if let Some(value) = self.kv_block_size {
            config.kv_cache.block_size = value;
        }
        if let Some(value) = self.kv_blocks {
            config.kv_cache.block_count = value;
        }
        if let Some(value) = self.kv_cache_dtype {
            config.kv_cache.dtype = value;
        }
        if let Some(value) = self.max_batch_requests {
            config.scheduler.max_batch_requests = value;
        }
        if let Some(value) = self.max_batch_tokens {
            config.scheduler.max_batch_tokens = value;
        }
        config
    }
}

pub(super) fn validate_schema(version: u32) -> Result<()> {
    if version == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(Error::Config(format!(
            "unsupported schema_version {version}; expected {SCHEMA_VERSION}"
        )))
    }
}
