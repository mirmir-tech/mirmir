//! Installed models, found by the model a reference names: its repository
//! and a variant's label in any case, as the Hub, the repository's listing
//! and the transfer registry match them. Its configuration is saved under
//! the spelling it was pulled by, the one name every file system agrees on.

use super::{
    super::{HubReference, ModelConfig, Store},
    Keys,
};
use crate::error::{Error, Result};

/// The downloaded Hub models, read once for many membership checks.
#[derive(Debug, Default)]
pub struct Downloaded(Vec<HubReference>);

impl Downloaded {
    /// Whether the model `reference` names is downloaded.
    #[must_use]
    pub fn contains(&self, reference: &HubReference) -> bool {
        self.0.iter().any(|downloaded| downloaded.is_same_model(reference))
    }
}

/// The Hub reference `model` was saved from.
fn saved_from(model: &ModelConfig) -> Option<HubReference> {
    model.hub.as_ref()?.repo_id.parse().ok()
}

impl Store {
    /// Whether the model `repo_id` names is downloaded. For many models,
    /// `downloaded_models` reads the configurations once.
    pub fn hub_model_downloaded(&self, repo_id: &str) -> Result<bool> {
        let wanted = repo_id.parse::<HubReference>()?;
        Ok(self.installed(&wanted)?.is_some_and(|model| model.path.exists()))
    }

    /// The downloaded Hub models.
    pub fn downloaded_models(&self) -> Result<Downloaded> {
        Ok(Downloaded(
            self.list_model_configs()?
                .iter()
                .filter(|model| model.path.exists())
                .filter_map(saved_from)
                .collect(),
        ))
    }

    /// The configuration of the model `wanted` names: under its key, else,
    /// for a model spelled otherwise, under the saved spelling.
    fn installed(&self, wanted: &HubReference) -> Result<Option<ModelConfig>> {
        let same = |model: &ModelConfig| {
            saved_from(model).is_some_and(|saved| saved.is_same_model(wanted))
        };
        match self.load_model_config(&wanted.key()) {
            Ok(model) => return Ok(same(&model).then_some(model)),
            Err(Error::ModelNotFound(_)) => {},
            // Its own configuration, unreadable, installs nothing: a pull
            // saves it anew.
            Err(error) => {
                tracing::warn!(model = %wanted, %error, "model configuration cannot be read");
            },
        }
        Ok(self.list_model_configs()?.into_iter().find(same))
    }

    /// The configuration of the Hub model keyed `key` saved under another
    /// spelling of it, as the Hub reference it keys is matched.
    pub(super) fn spelled_hub_model(&self, key: &str, keys: Keys) -> Result<Option<ModelConfig>> {
        Ok(self
            .model_configs(keys)?
            .into_iter()
            .find(|model| model.hub.is_some() && model.id.eq_ignore_ascii_case(key)))
    }
}
