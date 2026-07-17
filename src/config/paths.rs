use std::{env, path::PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub config_file: PathBuf,
    pub secrets_file: PathBuf,
    pub models_dir: PathBuf,
    pub ux_state_file: PathBuf,
    pub telemetry_file: PathBuf,
    pub hub_cache_dir: PathBuf,
    pub socket_file: PathBuf,
    pub lock_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| Error::Config("HOME is not set".to_owned()))?;
        let config_dir = env::var_os("MIRMIR_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("XDG_CONFIG_HOME").map(|root| PathBuf::from(root).join("mirmir"))
            })
            .unwrap_or_else(|| home.join(".config/mirmir"));
        let state_dir = env::var_os("MIRMIR_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("XDG_STATE_HOME").map(|root| PathBuf::from(root).join("mirmir"))
            })
            .unwrap_or_else(|| home.join(".local/state/mirmir"));
        let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .map_or_else(|| state_dir.clone(), |root| root.join("mirmir"));
        Ok(Self::from_roots(config_dir, state_dir, &runtime_dir))
    }

    #[must_use]
    pub fn from_roots(
        config_dir: PathBuf,
        state_dir: PathBuf,
        runtime_dir: &std::path::Path,
    ) -> Self {
        Self {
            config_file: config_dir.join("config.toml"),
            secrets_file: config_dir.join("secrets.toml"),
            models_dir: config_dir.join("models"),
            ux_state_file: config_dir.join("state.toml"),
            telemetry_file: state_dir.join("telemetry.toml"),
            hub_cache_dir: state_dir.join("hf-hub"),
            socket_file: runtime_dir.join("mirmir.sock"),
            lock_file: runtime_dir.join("mirmir.lock"),
            config_dir,
            state_dir,
        }
    }

    pub fn ensure_config_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.models_dir)?;
        set_private_dir(&self.config_dir)?;
        Ok(())
    }

    pub fn ensure_runtime_dirs(&self) -> Result<()> {
        if let Some(runtime_dir) = self.socket_file.parent() {
            std::fs::create_dir_all(runtime_dir)?;
            set_private_dir(runtime_dir)?;
        }
        std::fs::create_dir_all(&self.state_dir)?;
        set_private_dir(&self.state_dir)?;
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_dir(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir(_path: &std::path::Path) -> Result<()> {
    Ok(())
}
