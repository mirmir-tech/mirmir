//! GGUF models load through an import: a directory with the configuration,
//! tokenizer and generation settings derived from the file's metadata and a
//! link to the file itself, which then loads like a `SafeTensors` checkpoint.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use libmir::{
    WeightContainer,
    gguf::{GgufModel, Import, files_in, projector, shard_paths},
};

use super::{Store, folder::named_key, folder_key};
use crate::error::Result;

/// A GGUF source: the text model's file (its first shard) and an optional
/// vision tower's projector beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    text: PathBuf,
    vision: Option<PathBuf>,
}

impl Store {
    /// The checkpoint `path` loads from: its GGUF import, or `path` itself.
    pub fn checkpoint(&self, path: &Path) -> Result<PathBuf> {
        let canonical = fs::canonicalize(path)?;
        Ok(self.gguf_import(&canonical)?.unwrap_or(canonical))
    }

    /// The directory a GGUF `path` loads from, imported or refreshed only
    /// when the import is missing or older than its source; `None` when
    /// `path` is not a GGUF model. `path` is canonical: a `.gguf` file, or a
    /// directory of GGUF files without a `config.json`.
    pub(crate) fn gguf_import(&self, path: &Path) -> Result<Option<PathBuf>> {
        let Some(source) = source(path)? else {
            return Ok(None);
        };
        let directory = self.gguf_import_dir(path);
        // Earlier versions keyed a file by its path without the extension and
        // cut a directory's name at its first dot.
        let legacy = self.paths.gguf_imports_dir.join(folder_key(&path.with_extension("")));
        adopt(&legacy, &directory, &source)?;
        if !current(&directory, &source)? {
            write(&source, &directory)?;
        }
        Ok(Some(directory))
    }

    /// Where a canonical GGUF `path` imports to: a file under its stem; a
    /// directory under its whole name, whose dots (`Qwen2.5`) are not an
    /// extension. Both are told apart by a digest of the whole path.
    pub(crate) fn gguf_import_dir(&self, path: &Path) -> PathBuf {
        let key = if path.is_file() {
            named_key(path.file_stem(), path)
        } else {
            folder_key(path)
        };
        self.paths.gguf_imports_dir.join(key)
    }
}

fn source(path: &Path) -> libmir::Result<Option<Source>> {
    if path.is_file() {
        return Ok((WeightContainer::of(path) == Some(WeightContainer::Gguf))
            .then(|| Source { text: path.to_path_buf(), vision: None }));
    }
    if !path.is_dir() || path.join("config.json").exists() {
        return Ok(None);
    }
    // The header, not the file name, tells the projector from the text model.
    let vision = projector(path)?;
    Ok(files_in(path)?
        .into_iter()
        .find(|file| Some(file) != vision.as_ref())
        .map(|text| Source { text, vision }))
}

/// Moves the `legacy` import of `source`, made under an earlier key, to
/// `directory`, or removes it when `directory` already exists. A `legacy`
/// directory importing another source (a file `Qwen2.gguf` beside the
/// directory `Qwen2.5`) is left alone.
fn adopt(legacy: &Path, directory: &Path, source: &Source) -> Result<()> {
    if legacy == directory || !imports(legacy, &source.text) {
        return Ok(());
    }
    if directory.exists() {
        fs::remove_dir_all(legacy)?;
    } else {
        fs::rename(legacy, directory)?;
    }
    Ok(())
}

/// Whether `directory` is an import linking the GGUF file `text`.
fn imports(directory: &Path, text: &Path) -> bool {
    let (Some(name), Ok(text)) = (text.file_name(), fs::canonicalize(text)) else {
        return false;
    };
    fs::canonicalize(directory.join(name)).is_ok_and(|linked| linked == text)
}

/// Whether `directory` holds a complete import newer than every source file:
/// one linking each shard of the text model and of its vision tower.
fn current(directory: &Path, source: &Source) -> Result<bool> {
    let Ok(imported) = fs::metadata(directory.join("config.json")).and_then(|meta| meta.modified())
    else {
        return Ok(false);
    };
    let files = source.files()?;
    if !files.iter().all(|file| imports(directory, file)) {
        return Ok(false);
    }
    let newest = files
        .iter()
        .map(|file| fs::metadata(file).and_then(|meta| meta.modified()))
        .collect::<std::io::Result<Vec<SystemTime>>>()?
        .into_iter()
        .max();
    Ok(newest.is_some_and(|newest| imported >= newest))
}

impl Source {
    /// Every file the import links: each shard of the text model, then of
    /// the vision tower.
    fn files(&self) -> libmir::Result<Vec<PathBuf>> {
        let mut files = shard_paths(&self.text)?;
        if let Some(vision) = &self.vision {
            files.extend(shard_paths(vision)?);
        }
        Ok(files)
    }
}

/// Publishes the import whole: the previous one stays until every file of
/// the new one is written.
fn write(source: &Source, directory: &Path) -> libmir::Result<()> {
    let text = GgufModel::open(&source.text)?;
    let mut import = Import::derive(&text)?;
    if let Some(vision) = &source.vision {
        import = import.with_vision(&GgufModel::open(vision)?, &text)?;
    }
    import.publish(directory)?;
    Ok(())
}

#[cfg(test)]
mod tests;
