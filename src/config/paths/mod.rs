use std::{env, path::PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub enum SocketAccess {
    Owner,
    Group,
}

#[derive(Debug, Clone, Copy)]
enum SocketPlacement {
    Personal,
    External(SocketAccess),
}

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
    socket_placement: SocketPlacement,
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
            socket_placement: SocketPlacement::Personal,
        }
    }

    pub fn with_socket(mut self, socket: PathBuf, access: SocketAccess) -> Result<Self> {
        let parent = socket.parent().filter(|parent| !parent.as_os_str().is_empty());
        let parent =
            parent.ok_or_else(|| Error::Config("socket needs a parent directory".to_owned()))?;
        if socket.file_name().is_none() {
            return Err(Error::Config("socket needs a file name".to_owned()));
        }
        if matches!(access, SocketAccess::Group)
            && (parent == self.state_dir || parent == self.config_dir)
        {
            return Err(Error::Config(
                "a shared socket needs a separate runtime directory".to_owned(),
            ));
        }
        let name = socket.file_name().unwrap_or_default().to_string_lossy();
        self.lock_file =
            socket.with_file_name(format!("{}.lock", name.strip_suffix(".sock").unwrap_or(&name)));
        self.socket_file = socket;
        self.socket_placement = SocketPlacement::External(access);
        Ok(self)
    }

    pub const fn socket_access(&self) -> SocketAccess {
        match self.socket_placement {
            SocketPlacement::Personal => SocketAccess::Owner,
            SocketPlacement::External(access) => access,
        }
    }

    pub fn ensure_config_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.models_dir)?;
        set_private_dir(&self.config_dir)?;
        Ok(())
    }

    pub fn ensure_runtime_dirs(&self) -> Result<()> {
        if let Some(runtime_dir) = self.socket_file.parent() {
            let existed = runtime_dir.exists();
            std::fs::create_dir_all(runtime_dir)?;
            match self.socket_placement {
                SocketPlacement::Personal => set_private_dir(runtime_dir)?,
                SocketPlacement::External(access) if !existed => {
                    set_dir_permissions(runtime_dir, access)?;
                },
                SocketPlacement::External(_) => {},
            }
        }
        std::fs::create_dir_all(&self.state_dir)?;
        set_private_dir(&self.state_dir)?;
        Ok(())
    }
}

#[cfg(unix)]
fn set_dir_permissions(path: &std::path::Path, access: SocketAccess) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = match access {
        SocketAccess::Owner => 0o700,
        SocketAccess::Group => 0o750,
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_dir_permissions(_path: &std::path::Path, _access: SocketAccess) -> Result<()> {
    Ok(())
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

#[cfg(test)]
mod tests;
