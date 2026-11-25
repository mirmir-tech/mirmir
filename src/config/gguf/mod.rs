//! GGUF models load through an import: a directory with the configuration,
//! tokenizer and generation settings derived from the file's metadata and a
//! link to the file itself, which then loads like a `SafeTensors` checkpoint.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use libmir::{
    WeightContainer,
    gguf::{GgufHeader, GgufModel, Import, ImportManifest, files_in, shard_paths},
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
    /// when its published record no longer matches the source; `None` when
    /// `path` is not a GGUF model. `path` is canonical: a `.gguf` file, or a
    /// directory of GGUF files without a `config.json`. A current import is
    /// told by file names and metadata, without reading a GGUF header.
    pub(crate) fn gguf_import(&self, path: &Path) -> Result<Option<PathBuf>> {
        let directory = self.gguf_import_dir(path);
        if current(&directory, path) {
            return Ok(Some(directory));
        }
        let Some(source) = source(path)? else {
            return Ok(None);
        };
        // Earlier versions keyed a file by its path without the extension and
        // cut a directory's name at its first dot.
        let legacy = self.paths.gguf_imports_dir.join(folder_key(&path.with_extension("")));
        // A migration only: a legacy import that cannot move stays behind.
        if let Err(error) = adopt(&legacy, &directory, &source) {
            tracing::warn!(legacy = %legacy.display(), %error, "keeping a legacy GGUF import");
        }
        write(&source, &directory)?;
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
    // A file whose header cannot be read is skipped, unless no text model is
    // readable at all.
    let (mut text, mut vision, mut unreadable) = (None, None, None);
    for file in files_in(path)? {
        match GgufHeader::read(&file) {
            Ok(header) if header.is_projector() => {
                vision.get_or_insert(file);
            },
            Ok(_) => {
                text.get_or_insert(file);
            },
            Err(error) => {
                tracing::warn!(file = %file.display(), %error, "skipping an unreadable GGUF file");
                unreadable.get_or_insert(error);
            },
        }
    }
    match (text, unreadable) {
        (Some(text), _) => Ok(Some(Source { text, vision })),
        (None, Some(error)) => Err(error.into()),
        (None, None) => Ok(None),
    }
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

/// Whether `directory` holds a published import of the GGUF `path` that is
/// still current: every file it recorded is there, each source is unchanged,
/// and `path` still groups into those sources, the shards of a file or the
/// GGUF files of a directory without a `config.json`.
fn current(directory: &Path, path: &Path) -> bool {
    let Some(manifest) = ImportManifest::read(directory) else {
        return false;
    };
    let files = if path.is_file() {
        if !manifest.vision.is_empty() {
            return false;
        }
        shard_paths(path)
    } else if path.join("config.json").exists() {
        return false;
    } else {
        files_in(path)
    };
    let Ok(files) = files else {
        return false;
    };
    let sources = files.iter().map(fs::canonicalize).collect::<std::io::Result<BTreeSet<_>>>();
    let recorded = manifest.sources().map(|source| source.path.clone()).collect::<BTreeSet<_>>();
    sources.is_ok_and(|sources| sources == recorded) && manifest.is_current(directory)
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
