use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::{
    HubReference, RemovalScope, Store, file::write_toml, schema::SCHEMA_VERSION,
    store::StateRecovery,
};
use crate::error::{Error, Result};

const RECENT_LIMIT: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UxState {
    schema_version: u32,
    recent_models: Vec<String>,
    active_models: Vec<String>,
}

impl Default for UxState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            recent_models: Vec::new(),
            active_models: Vec::new(),
        }
    }
}

impl Store {
    pub fn remember_model(&self, id: &str) -> Result<()> {
        self.paths.ensure_config_dirs()?;
        let _guard = self.lock_state()?;
        let mut state = self.load_ux_state()?;
        state.recent_models.retain(|recent| recent != id);
        state.recent_models.insert(0, id.to_owned());
        state.recent_models.truncate(RECENT_LIMIT);
        write_toml(&self.paths.ux_state_file, &state, false)
    }

    pub fn recent_models(&self) -> Result<Vec<String>> {
        let _guard = self.lock_state()?;
        Ok(self.load_ux_state()?.recent_models)
    }

    pub fn active_models(&self) -> Result<Vec<String>> {
        let _guard = self.lock_state()?;
        let mut state = self.load_ux_state()?;
        if self.normalize_active_models(&mut state) {
            write_toml(&self.paths.ux_state_file, &state, false)?;
        }
        Ok(state.active_models)
    }

    pub fn activate_model(&self, id: &str) -> Result<()> {
        self.paths.ensure_config_dirs()?;
        let _guard = self.lock_state()?;
        let mut state = self.load_ux_state()?;
        self.normalize_active_models(&mut state);
        if !state.active_models.iter().any(|active| active == id) {
            state.active_models.push(id.to_owned());
            write_toml(&self.paths.ux_state_file, &state, false)?;
        }
        Ok(())
    }

    pub fn deactivate_model(&self, id: &str) -> Result<()> {
        self.deactivate(|active| active == id)
    }

    /// Deactivates every model the removal `scope` took: one it names in
    /// any spelling, a model or a repository with each of its variants, and
    /// one activated by its path within a cache it deleted, the files gone.
    pub fn deactivate_models_of(&self, scope: &RemovalScope) -> Result<()> {
        let asked = scope.asked();
        self.deactivate(|active| {
            let path = Path::new(active);
            asked.owns_key(active)
                || (scope.holds_path(path) && !path.exists())
                || active.parse::<HubReference>().is_ok_and(|reference| match asked.variant() {
                    Some(_) => asked.is_same_model(&reference),
                    None => asked.same_repository(&reference),
                })
        })
    }

    fn deactivate(&self, gone: impl Fn(&str) -> bool) -> Result<()> {
        self.paths.ensure_config_dirs()?;
        let _guard = self.lock_state()?;
        let mut state = self.load_ux_state()?;
        let previous = state.active_models.clone();
        self.normalize_active_models(&mut state);
        state.active_models.retain(|active| !gone(active));
        if state.active_models != previous {
            write_toml(&self.paths.ux_state_file, &state, false)?;
        }
        Ok(())
    }

    fn normalize_active_models(&self, state: &mut UxState) -> bool {
        let mut normalized = Vec::with_capacity(state.active_models.len());
        for selector in &state.active_models {
            // `lock_state` moved every legacy variant before this lock.
            let key = self
                .select(selector, super::models::Keys::Migrated)
                .map_or_else(|_| selector.clone(), |model| model.key);
            if !normalized.contains(&key) {
                normalized.push(key);
            }
        }
        if normalized == state.active_models {
            return false;
        }
        state.active_models = normalized;
        true
    }

    fn load_ux_state(&self) -> Result<UxState> {
        if !self.paths.ux_state_file.exists() {
            return Ok(UxState::default());
        }
        let contents = fs::read_to_string(&self.paths.ux_state_file)?;
        match toml::from_str::<UxState>(&contents) {
            Ok(state) => match super::schema::validate_schema(state.schema_version) {
                Ok(()) => Ok(state),
                Err(error) => self.recover_ux_state(&error.to_string()),
            },
            Err(error) => self.recover_ux_state(&error.to_string()),
        }
    }

    fn recover_ux_state(&self, reason: &str) -> Result<UxState> {
        self.paths.ensure_config_dirs()?;
        let original = self.paths.ux_state_file.clone();
        let quarantine = self.quarantine_path();
        fs::rename(&original, &quarantine)?;
        let state = UxState::default();
        write_toml(&original, &state, false)?;
        let recovery = StateRecovery {
            original: original.clone(),
            quarantine: quarantine.clone(),
            reason: reason.to_owned(),
        };
        *self.recovery_lock()? = Some(recovery);
        tracing::warn!(
            file = %original.display(),
            quarantine = %quarantine.display(),
            error = %reason,
            "corrupt managed state quarantined and reset"
        );
        Ok(state)
    }

    fn quarantine_path(&self) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis());
        self.paths
            .config_dir
            .join(format!("state.corrupt-{timestamp}-{}.toml", std::process::id()))
    }

    /// Holds the UX state for one read or write. Legacy variant keys move
    /// first, renaming the selections that name them, so nothing resolves
    /// a selection while its key may still change, and no migration runs
    /// under this lock.
    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.migrate_variant_keys()?;
        self.ux_state_guard()
    }

    fn ux_state_guard(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        let Ok(state) = self.ux_state_lock.lock() else {
            return Err(Error::Config("UX state lock is poisoned".to_owned()));
        };
        Ok(state)
    }

    /// Renames the recent and active selections of the configuration `from`
    /// to `to`, while `from` still names the model that moved: a namesake
    /// saved there later is not the selected one.
    pub(super) fn rename_selections(&self, from: &str, to: &str) -> Result<()> {
        if !self.paths.ux_state_file.exists() {
            return Ok(());
        }
        let _guard = self.ux_state_guard()?;
        let mut state = self.load_ux_state()?;
        let mut renamed = false;
        for id in state.recent_models.iter_mut().chain(&mut state.active_models) {
            if id == from {
                to.clone_into(id);
                renamed = true;
            }
        }
        if renamed {
            write_toml(&self.paths.ux_state_file, &state, false)?;
        }
        Ok(())
    }

    fn recovery_lock(&self) -> Result<std::sync::MutexGuard<'_, Option<StateRecovery>>> {
        let Ok(recovery) = self.ux_state_recovery.lock() else {
            return Err(Error::Config("UX state recovery lock is poisoned".to_owned()));
        };
        Ok(recovery)
    }

    pub(crate) fn take_state_recovery(&self) -> Result<Option<StateRecovery>> {
        Ok(self.recovery_lock()?.take())
    }
}

#[cfg(test)]
mod tests;
