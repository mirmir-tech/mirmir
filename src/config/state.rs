use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use super::{Store, file::write_toml, schema::SCHEMA_VERSION, store::StateRecovery};
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
        self.paths.ensure_config_dirs()?;
        let _guard = self.lock_state()?;
        let mut state = self.load_ux_state()?;
        let previous = state.active_models.clone();
        self.normalize_active_models(&mut state);
        state.active_models.retain(|active| active != id);
        if state.active_models != previous {
            write_toml(&self.paths.ux_state_file, &state, false)?;
        }
        Ok(())
    }

    fn normalize_active_models(&self, state: &mut UxState) -> bool {
        let mut normalized = Vec::with_capacity(state.active_models.len());
        for selector in &state.active_models {
            let key = self
                .resolve_model(selector)
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

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.ux_state_lock
            .lock()
            .map_err(|_| Error::Config("UX state lock is poisoned".to_owned()))
    }

    fn recovery_lock(&self) -> Result<std::sync::MutexGuard<'_, Option<StateRecovery>>> {
        self.ux_state_recovery
            .lock()
            .map_err(|_| Error::Config("UX state recovery lock is poisoned".to_owned()))
    }

    pub(crate) fn take_state_recovery(&self) -> Result<Option<StateRecovery>> {
        Ok(self.recovery_lock()?.take())
    }
}

#[cfg(test)]
mod tests;
