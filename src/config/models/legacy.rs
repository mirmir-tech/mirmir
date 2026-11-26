//! Variant configurations saved before a variant's key joined it to its
//! repository with `@`, as `owner--name--VARIANT`. They move to the current
//! key together with the variant's view and pending marker, named after the
//! key too, so loading, listing and removing a variant downloaded earlier
//! keep working, and a namesake repository (`owner/name--VARIANT`) whose
//! key the legacy one is never takes its place.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use super::{
    super::{HubReference, ModelConfig, Store, file::write_toml, folder_key},
    Keys,
};
use crate::error::Result;

/// The key the variant keyed `key` (`owner--name@VARIANT`) had before:
/// `owner--name--VARIANT`; `None` for a key that is no variant's.
pub(super) fn legacy_key(key: &str) -> Option<String> {
    key.contains('@').then(|| key.replacen('@', "--", 1))
}

impl Store {
    /// Moves every legacy variant configuration to its current key. It runs
    /// without the configuration write lock, which a caller may hold, so
    /// each step tolerates another migration having made it first.
    pub(in crate::config) fn migrate_variant_keys(&self) -> Result<()> {
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

    /// Moves the variant `reference` from its legacy key, if it is saved
    /// there, before anything looks for it by its current key.
    pub(crate) fn migrate_variant(&self, reference: &HubReference) -> Result<()> {
        match legacy_key(&reference.key()) {
            Some(legacy) => self.migrate_variant_key(&self.model_config_path(&legacy)?),
            None => Ok(()),
        }
    }

    /// The current id of the variant whose legacy key is `legacy`, an id a
    /// selection saved before it moved.
    pub(super) fn current_variant_id(&self, legacy: &str, keys: Keys) -> Result<Option<String>> {
        Ok(self.model_configs(keys)?.into_iter().find_map(|model| {
            let reference = model.hub?.repo_id.parse::<HubReference>().ok()?;
            (legacy_key(&reference.key()).as_deref() == Some(legacy)).then_some(model.id)
        }))
    }

    /// Moves the configuration at `path` to its own key when it records a
    /// variant saved under its legacy key, which may be a namesake's. Under
    /// any other name it stays: on a case-insensitive file system, another
    /// spelling of its own key names the configuration itself.
    pub(super) fn migrate_variant_key(&self, path: &Path) -> Result<()> {
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
        if legacy_key(&key).as_deref() != Some(stem) {
            return Ok(());
        }
        let target = self.model_config_path(&key)?;
        if same_file(path, &target) {
            return Ok(());
        }
        if !target.exists() {
            self.move_view(&mut model, stem, &key)?;
            let variants = &self.paths.gguf_variants_dir;
            let marker = |key: &str| variants.join(format!("{key}.pending"));
            if marker(stem).exists() {
                fs::rename(marker(stem), marker(&key))?;
            }
            model.id.clone_from(&key);
            write_toml(&target, &model, false)?;
        }
        // Selections of the legacy key follow before a namesake can take it.
        self.rename_selections(stem, &key)?;
        // The current key's configuration, new or found, supersedes it.
        match fs::remove_file(path) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    /// Moves `model`'s view, named `stem`, to `key` with its import, and
    /// points `model` at it, whether or not an interrupted migration moved
    /// the view already.
    fn move_view(&self, model: &mut ModelConfig, stem: &str, key: &str) -> Result<()> {
        let Ok(variants) = self.paths.gguf_variants_dir.canonicalize() else {
            return Ok(());
        };
        let (old_view, view) = (variants.join(stem), variants.join(key));
        if model.path != old_view && !same_file(&model.path, &old_view) {
            return Ok(());
        }
        if old_view.exists() && !view.exists() {
            fs::rename(&old_view, &view)?;
        }
        if !view.exists() {
            return Ok(());
        }
        // The view's import is keyed by the view's path: moved along, it is
        // not imported again beside a stale copy.
        let imports = [
            self.gguf_import_dir(&old_view),
            self.paths.gguf_imports_dir.join(folder_key(&old_view.with_extension(""))),
        ];
        move_import(&imports, &self.gguf_import_dir(&view))?;
        model.path = view;
        Ok(())
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
