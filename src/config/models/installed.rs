//! Installed models, found by the model a reference names: a variant by
//! its label in any case, as the repository's listing and the transfer
//! registry match it. Its configuration is saved under the label's own
//! spelling, the one name every file system agrees on.

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

    /// The reference the installed model `wanted` names was saved from.
    pub(crate) fn installed_reference(
        &self,
        wanted: &HubReference,
    ) -> Result<Option<HubReference>> {
        Ok(self.installed(wanted)?.as_ref().and_then(saved_from))
    }

    /// The configuration of the model `wanted` names: under its key, else,
    /// for a variant whose label is spelled otherwise, under the saved one.
    fn installed(&self, wanted: &HubReference) -> Result<Option<ModelConfig>> {
        let same = |model: &ModelConfig| {
            saved_from(model).is_some_and(|saved| saved.is_same_model(wanted))
        };
        match self.load_model_config(&wanted.key()) {
            Ok(model) => return Ok(same(&model).then_some(model)),
            Err(Error::ModelNotFound(_)) => {},
            Err(error) => return Err(error),
        }
        if wanted.variant().is_none() {
            return Ok(None);
        }
        Ok(self.list_model_configs()?.into_iter().find(same))
    }

    /// The configuration of the variant keyed `key` (`owner--name@VARIANT`)
    /// saved under another spelling of its label.
    pub(super) fn spelled_variant(&self, key: &str, keys: Keys) -> Result<Option<ModelConfig>> {
        let Some((repository, label)) = key.split_once('@') else {
            return Ok(None);
        };
        Ok(self.model_configs(keys)?.into_iter().find(|model| {
            model.id.split_once('@').is_some_and(|(saved, other)| {
                saved == repository && other.eq_ignore_ascii_case(label)
            })
        }))
    }
}
