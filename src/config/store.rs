use std::{
    fs,
    sync::{Arc, Mutex},
};

use super::{
    AppConfig, Paths,
    file::{ensure_private_file, write_text, write_toml},
    schema::{SecretsConfig, validate_schema},
};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Store {
    pub(super) paths: Paths,
    pub(super) ux_state_lock: Arc<Mutex<()>>,
    pub(super) ux_state_recovery: Arc<Mutex<Option<StateRecovery>>>,
}

#[derive(Debug, Clone)]
pub struct StateRecovery {
    pub original: std::path::PathBuf,
    pub quarantine: std::path::PathBuf,
    pub reason: String,
}

impl Store {
    #[must_use]
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            ux_state_lock: Arc::new(Mutex::new(())),
            ux_state_recovery: Arc::new(Mutex::new(None)),
        }
    }

    #[must_use]
    pub const fn paths(&self) -> &Paths {
        &self.paths
    }

    pub fn initialize(&self) -> Result<()> {
        self.paths.ensure_config_dirs()?;
        if !self.paths.config_file.exists() {
            write_toml(&self.paths.config_file, &AppConfig::default(), false)?;
        }
        Ok(())
    }

    pub fn load(&self) -> Result<AppConfig> {
        if !self.paths.config_file.exists() {
            return Ok(AppConfig::default());
        }
        let config: AppConfig = toml::from_str(&fs::read_to_string(&self.paths.config_file)?)?;
        config.validate()?;
        Ok(config)
    }

    pub fn replace_config(&self, contents: &str) -> Result<()> {
        let config: AppConfig = toml::from_str(contents)?;
        config.validate()?;
        self.paths.ensure_config_dirs()?;
        write_text(&self.paths.config_file, contents, false)
    }

    pub fn set_hf_token(&self, token: &str) -> Result<()> {
        if token.is_empty() {
            return Err(Error::Config("Hugging Face token cannot be empty".to_owned()));
        }
        self.paths.ensure_config_dirs()?;
        let mut secrets = self.load_secrets()?;
        secrets.hugging_face.token = Some(token.to_owned());
        write_toml(&self.paths.secrets_file, &secrets, true)
    }

    pub fn remove_hf_token(&self) -> Result<()> {
        if !self.paths.secrets_file.exists() {
            return Ok(());
        }
        let mut secrets = self.load_secrets()?;
        secrets.hugging_face.token = None;
        write_toml(&self.paths.secrets_file, &secrets, true)
    }

    pub fn hf_token(&self) -> Result<Option<String>> {
        if let Ok(token) = std::env::var("HF_TOKEN") {
            return Ok((!token.is_empty()).then_some(token));
        }
        Ok(self.load_secrets()?.hugging_face.token)
    }

    pub fn set_http_api_key(&self, key: &str) -> Result<()> {
        if key.is_empty() {
            return Err(Error::Config("HTTP API key cannot be empty".to_owned()));
        }
        self.paths.ensure_config_dirs()?;
        let mut secrets = self.load_secrets()?;
        secrets.server.api_key = Some(key.to_owned());
        write_toml(&self.paths.secrets_file, &secrets, true)
    }

    pub fn remove_http_api_key(&self) -> Result<()> {
        if !self.paths.secrets_file.exists() {
            return Ok(());
        }
        let mut secrets = self.load_secrets()?;
        secrets.server.api_key = None;
        write_toml(&self.paths.secrets_file, &secrets, true)
    }

    pub fn http_api_key(&self) -> Result<Option<String>> {
        if let Ok(key) = std::env::var("MIRMIR_HTTP_API_KEY") {
            return Ok((!key.is_empty()).then_some(key));
        }
        Ok(self.load_secrets()?.server.api_key)
    }

    pub(super) fn load_secrets(&self) -> Result<SecretsConfig> {
        if !self.paths.secrets_file.exists() {
            return Ok(SecretsConfig::default());
        }
        ensure_private_file(&self.paths.secrets_file)?;
        let secrets: SecretsConfig =
            toml::from_str(&fs::read_to_string(&self.paths.secrets_file)?)?;
        validate_schema(secrets.schema_version)?;
        Ok(secrets)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::config::schema::SCHEMA_VERSION;

    static NEXT_STORE: AtomicU64 = AtomicU64::new(0);

    fn store() -> Store {
        let unique = NEXT_STORE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("mirmir-config-{}-{unique}", std::process::id()));
        Store::new(Paths::from_roots(
            root.join("config"),
            root.join("state"),
            &root.join("runtime"),
        ))
    }

    #[test]
    fn initializes_and_loads_versioned_config() -> Result<()> {
        let store = store();
        store.initialize()?;
        let config = store.load()?;
        assert_eq!(config.schema_version, SCHEMA_VERSION);
        assert!(store.paths.config_file.exists());
        Ok(())
    }

    #[test]
    fn saves_and_redacts_hf_token() -> Result<()> {
        let store = store();
        store.set_hf_token("hf_secret")?;
        assert_eq!(store.load_secrets()?.hugging_face.token.as_deref(), Some("hf_secret"));
        let visible = serde_json::to_string(&store.configuration()?)?;
        assert!(!visible.contains("hf_secret"));
        assert!(visible.contains("configured"));
        Ok(())
    }

    #[test]
    fn saves_and_redacts_http_api_key() -> Result<()> {
        let store = store();
        store.set_http_api_key("mirmir-secret")?;
        assert_eq!(store.load_secrets()?.server.api_key.as_deref(), Some("mirmir-secret"));
        let visible = serde_json::to_string(&store.configuration()?)?;
        assert!(!visible.contains("mirmir-secret"));
        assert!(visible.contains("http_api_key"));
        Ok(())
    }

    #[test]
    fn rejects_unknown_schema() -> Result<()> {
        let store = store();
        store.paths.ensure_config_dirs()?;
        fs::write(&store.paths.config_file, "schema_version = 99\n")?;
        assert!(store.load().is_err());
        Ok(())
    }

    #[test]
    fn replaces_config_only_after_validation() -> Result<()> {
        let store = store();
        store.initialize()?;
        let original = fs::read_to_string(&store.paths.config_file)?;
        assert!(store.replace_config("schema_version = 99\n").is_err());
        assert_eq!(fs::read_to_string(&store.paths.config_file)?, original);
        store.replace_config(&original.replace("web_enabled = false", "web_enabled = true"))?;
        assert!(store.load()?.server.web_enabled);
        Ok(())
    }
}
