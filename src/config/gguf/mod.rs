//! GGUF models load through an import: a directory with the configuration,
//! tokenizer and generation settings derived from the file's metadata and a
//! link to the file itself, which then loads like a `SafeTensors` checkpoint.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use libmir::gguf::{GgufModel, Import, files_in};

use super::{Store, folder_key};
use crate::error::Result;

/// A GGUF source: the text model's file (its first shard) and an optional
/// `mmproj` vision tower beside it.
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

    /// The directory a GGUF `path` loads from, imported or refreshed when
    /// the import is missing or older than its source; `None` when `path` is
    /// not a GGUF model. `path` is canonical: a `.gguf` file, or a directory
    /// of GGUF files without a `config.json`.
    pub(crate) fn gguf_import(&self, path: &Path) -> Result<Option<PathBuf>> {
        let Some(source) = source(path)? else {
            return Ok(None);
        };
        // A file imports under its stem; a directory keeps its whole name,
        // whose dots (`Qwen2.5`) are not an extension.
        let name = if path.is_file() {
            path.with_extension("")
        } else {
            path.to_path_buf()
        };
        let directory = self.paths.gguf_imports_dir.join(folder_key(&name));
        if !current(&directory, &source)? {
            write(&source, &directory)?;
        }
        Ok(Some(directory))
    }
}

fn source(path: &Path) -> libmir::Result<Option<Source>> {
    if path.is_file() {
        return Ok(is_gguf(path).then(|| Source { text: path.to_path_buf(), vision: None }));
    }
    if !path.is_dir() || path.join("config.json").exists() {
        return Ok(None);
    }
    let (vision, text): (Vec<_>, Vec<_>) =
        files_in(path)?.into_iter().partition(|file| is_mmproj(file));
    Ok(text
        .into_iter()
        .next()
        .map(|text| Source { text, vision: vision.into_iter().next() }))
}

fn is_gguf(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
}

/// llama.cpp names a vision tower `mmproj-*.gguf`.
fn is_mmproj(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().to_ascii_lowercase().starts_with("mmproj"))
}

/// Whether `directory` holds an import newer than every source file.
fn current(directory: &Path, source: &Source) -> Result<bool> {
    let Ok(imported) = fs::metadata(directory.join("config.json")).and_then(|meta| meta.modified())
    else {
        return Ok(false);
    };
    let newest = [Some(&source.text), source.vision.as_ref()]
        .into_iter()
        .flatten()
        .map(|file| fs::metadata(file).and_then(|meta| meta.modified()))
        .collect::<std::io::Result<Vec<SystemTime>>>()?
        .into_iter()
        .max();
    Ok(newest.is_some_and(|newest| imported >= newest))
}

fn write(source: &Source, directory: &Path) -> libmir::Result<()> {
    let text = GgufModel::open(&source.text)?;
    let mut import = Import::derive(&text)?;
    if let Some(vision) = &source.vision {
        import = import.with_vision(&GgufModel::open(vision)?, &text)?;
    }
    import.write(directory)?;
    Ok(())
}

#[cfg(test)]
mod tests;
