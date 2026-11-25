//! Variant configurations saved before a variant's key joined it to its
//! repository with `@`, as `owner--name--VARIANT`. They move to the current
//! key together with the variant's view and pending marker, named after the
//! key too, so loading, listing and removing a variant downloaded earlier
//! keep working.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use super::super::{HubReference, ModelConfig, Store, file::write_toml, folder_key};
use crate::error::Result;

impl Store {
    /// Moves every legacy variant configuration to its current key. It runs
    /// without the configuration write lock, which a caller may hold, so
    /// each step tolerates another listing having made it first.
    pub(super) fn migrate_variant_keys(&self) -> Result<()> {
        let Ok(entries) = fs::read_dir(&self.paths.models_dir) else {
            return Ok(());
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.extension().and_then(|value| value.to_str()) == Some("toml") {
                self.migrate_variant_key(&path)?;
            }
        }
        Ok(())
    }

    fn migrate_variant_key(&self, path: &Path) -> Result<()> {
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            return Ok(());
        };
        let Ok(text) = fs::read_to_string(path) else {
            return Ok(());
        };
        let Ok(mut model) = toml::from_str::<ModelConfig>(&text) else {
            return Ok(());
        };
        let Some(reference) = model
            .hub
            .as_ref()
            .and_then(|hub| hub.repo_id.parse::<HubReference>().ok())
            .filter(|reference| reference.variant().is_some())
        else {
            return Ok(());
        };
        let key = reference.key();
        if key == stem {
            return Ok(());
        }
        let target = self.model_config_path(&key);
        if !target.exists() {
            let variants = &self.paths.gguf_variants_dir;
            let (old_view, view) = (variants.join(stem), variants.join(&key));
            if same_file(&model.path, &old_view) && !view.exists() {
                let old_view = old_view.canonicalize()?;
                // The view's import is keyed by the view's path: moved along,
                // it is not imported again beside a stale copy.
                let imports = [
                    self.gguf_import_dir(&old_view),
                    self.paths.gguf_imports_dir.join(folder_key(&old_view.with_extension(""))),
                ];
                fs::rename(&old_view, &view)?;
                model.path = view.canonicalize()?;
                move_import(&imports, &self.gguf_import_dir(&model.path))?;
            }
            let marker = |key: &str| variants.join(format!("{key}.pending"));
            if marker(stem).exists() {
                fs::rename(marker(stem), marker(&key))?;
            }
            model.id = key;
            write_toml(&target, &model, false)?;
        }
        // The current key's configuration, new or found, supersedes it.
        match fs::remove_file(path) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}

/// Moves the first of a renamed view's `imports` found to `target`, unless
/// one is there already, and drops the rest: an import is derived from its
/// sources and made again when missing.
fn move_import(imports: &[PathBuf], target: &Path) -> Result<()> {
    for import in imports.iter().filter(|import| import.as_path() != target) {
        if !import.is_dir() {
            continue;
        }
        if target.exists() {
            fs::remove_dir_all(import)?;
        } else {
            fs::rename(import, target)?;
        }
    }
    Ok(())
}

fn same_file(left: &Path, right: &Path) -> bool {
    left.canonicalize().is_ok_and(|left| right.canonicalize().ok() == Some(left))
}
