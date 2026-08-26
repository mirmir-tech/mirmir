use super::{Application, Result, RuntimeCoordinator};
use crate::config::ConfigPresentation;

pub enum ConfigurationChange {
    SetValue { key: String, value: String },
    SetHfToken(String),
    RemoveHfToken,
    TestHfToken,
    SetHttpApiKey(String),
    RemoveHttpApiKey,
}

pub struct ConfigurationOutcome {
    pub configuration: ConfigPresentation,
    pub message: String,
    pub restart_required: bool,
}

impl Application {
    pub fn configuration(&self) -> Result<ConfigPresentation> {
        self.runtime.configuration()
    }

    pub async fn update_configuration(
        &self,
        change: ConfigurationChange,
    ) -> Result<ConfigurationOutcome> {
        self.runtime.update_configuration(change).await
    }
}

impl RuntimeCoordinator {
    pub fn configuration(&self) -> Result<ConfigPresentation> {
        Ok(self.store.configuration()?)
    }

    pub async fn update_configuration(
        &self,
        change: ConfigurationChange,
    ) -> Result<ConfigurationOutcome> {
        let (message, restart_required) = match change {
            ConfigurationChange::SetValue { key, value } => {
                self.store.set_config_value(&key, &value)?;
                let restart = key != "default_model";
                tracing::info!(%key, restart_required = restart, "configuration updated");
                (format!("saved {key}"), restart)
            },
            ConfigurationChange::SetHfToken(token) => {
                self.store.set_hf_token(token.trim())?;
                tracing::info!("Hugging Face token updated");
                ("Hugging Face token saved".to_owned(), false)
            },
            ConfigurationChange::RemoveHfToken => {
                self.store.remove_hf_token()?;
                tracing::info!("Hugging Face token removed from secrets.toml");
                ("stored Hugging Face token removed".to_owned(), false)
            },
            ConfigurationChange::TestHfToken => {
                let identity = self.catalog.test_hf_token().await?;
                tracing::info!(%identity, "Hugging Face token verified");
                (format!("token valid for {identity}"), false)
            },
            ConfigurationChange::SetHttpApiKey(key) => {
                self.store.set_http_api_key(key.trim())?;
                tracing::info!("HTTP API key updated");
                ("HTTP API key saved".to_owned(), true)
            },
            ConfigurationChange::RemoveHttpApiKey => {
                self.store.remove_http_api_key()?;
                tracing::info!("HTTP API key removed from secrets.toml");
                ("stored HTTP API key removed".to_owned(), true)
            },
        };
        Ok(ConfigurationOutcome {
            configuration: self.configuration()?,
            message,
            restart_required,
        })
    }
}
