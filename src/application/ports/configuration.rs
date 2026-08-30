use crate::{
    application::{ConfigurationChange, Result, configuration::ConfigurationOutcome},
    config::ConfigPresentation,
};

#[tonic::async_trait]
pub trait ConfigurationPort: Send + Sync {
    fn configuration(&self) -> Result<ConfigPresentation>;

    async fn update_configuration(
        &self,
        change: ConfigurationChange,
    ) -> Result<ConfigurationOutcome>;
}
