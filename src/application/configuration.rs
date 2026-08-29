use super::{Application, Result};
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
        self.configuration.configuration()
    }

    pub async fn update_configuration(
        &self,
        change: ConfigurationChange,
    ) -> Result<ConfigurationOutcome> {
        self.configuration.update_configuration(change).await
    }
}
