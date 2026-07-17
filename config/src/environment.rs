use std::path::{Path, PathBuf};

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentLoad {
    pub path: PathBuf,
}

pub fn load_environment(root: impl AsRef<Path>) -> Result<Vec<EnvironmentLoad>> {
    let root = root.as_ref();
    let mut loaded = Vec::new();
    for name in [".env.local", ".env"] {
        let path = root.join(name);
        if path.exists() {
            dotenvy::from_path(&path)?;
            loaded.push(EnvironmentLoad { path });
        }
    }
    Ok(loaded)
}
