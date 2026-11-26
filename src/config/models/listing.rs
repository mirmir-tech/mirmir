//! Listing the saved configurations, each read on its own: one that cannot
//! be read is set aside with its file and error, so it keeps no other model
//! from being found, listed or pulled, and no removal decides as if it were
//! not there.

use std::{fs, path::PathBuf};

use super::{
    super::{HubReference, ModelConfig, Store},
    Keys, read_model_config,
};
use crate::error::{Error, Result};

/// A configuration file that cannot be read, with why.
#[derive(Debug)]
struct Damaged {
    path: PathBuf,
    error: Error,
}

impl Store {
    /// The configurations that can be read, by id; each one that cannot is
    /// logged with its file.
    pub fn list_model_configs(&self) -> Result<Vec<ModelConfig>> {
        self.model_configs(Keys::Migrate)
    }

    pub(super) fn model_configs(&self, keys: Keys) -> Result<Vec<ModelConfig>> {
        let (models, damaged) = self.read_configs(keys)?;
        for damaged in damaged {
            tracing::warn!(
                file = %damaged.path.display(),
                error = %damaged.error,
                "model configuration skipped: it cannot be read"
            );
        }
        Ok(models)
    }

    /// The configurations that cannot be read a removal of `reference`
    /// takes as its own, by their key in any spelling. Refuses when another
    /// may be a model of its repository: a removal deciding by the
    /// repository's other models would take what that one holds.
    pub(crate) fn damaged_configs_of(&self, reference: &HubReference) -> Result<Vec<PathBuf>> {
        let mut own = Vec::new();
        for damaged in self.read_configs(Keys::Migrate)?.1 {
            let Some(stem) = damaged.path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if reference.owns_key(stem) {
                own.push(damaged.path);
            } else if reference.may_key(stem) {
                return Err(Error::Config(format!(
                    "`{}` may hold a model of `{}` but cannot be read ({}); repair or remove it first",
                    damaged.path.display(),
                    reference.repository(),
                    damaged.error
                )));
            }
        }
        Ok(own)
    }

    /// The configurations that can be read, by id, and those that cannot.
    fn read_configs(&self, keys: Keys) -> Result<(Vec<ModelConfig>, Vec<Damaged>)> {
        if !self.paths.models_dir.exists() {
            return Ok((Vec::new(), Vec::new()));
        }
        // Once per listing: every configuration below is under its own key.
        if keys == Keys::Migrate {
            self.migrate_variant_keys()?;
        }
        let (mut models, mut damaged) = (Vec::new(), Vec::new());
        for entry in fs::read_dir(&self.paths.models_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            let Some(key) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            match read_model_config(&path, key) {
                Ok(model) => models.push(model),
                Err(error) => damaged.push(Damaged { path, error }),
            }
        }
        models.sort_by(|left, right| left.id.cmp(&right.id));
        Ok((models, damaged))
    }
}
