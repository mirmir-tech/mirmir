//! Persistence of the key file: a fingerprint that detects any replacement,
//! a cross-process writer lock, and whole-file loads and saves.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use super::KeyRecord;
use crate::{
    config::{ensure_private_file, write_text},
    error::{Error, RemoteError, Result},
};

#[derive(Deserialize)]
struct KeyFile {
    keys: Vec<KeyRecord>,
}

#[derive(Serialize)]
struct RenderedKeyFile<'a> {
    keys: &'a [KeyRecord],
}

/// Identifies one version of the key file. Every save renames a new file into
/// place, so the inode changes even when the modification time does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stamp {
    modified: SystemTime,
    len: u64,
    file: u64,
}

impl Stamp {
    fn of(metadata: &fs::Metadata) -> Result<Self> {
        Ok(Self {
            modified: metadata.modified()?,
            len: metadata.len(),
            file: file_id(metadata),
        })
    }
}

/// The stamp of the key file on disk, or `None` when it does not exist.
pub(super) fn stamp(path: &Path) -> Result<Option<Stamp>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(Stamp::of(&metadata)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Reads the key file with the stamp of the version actually read.
pub(super) fn load(path: &Path) -> Result<(Option<Stamp>, Vec<KeyRecord>)> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((None, Vec::new()));
        },
        Err(error) => return Err(error.into()),
    };
    ensure_private_file(path)?;
    let stamp = Stamp::of(&file.metadata()?)?;
    let parsed: KeyFile = serde_json::from_reader(std::io::BufReader::new(file))?;
    Ok((Some(stamp), parsed.keys))
}

/// Atomically replaces the key file and returns the stamp of the new version.
/// The caller must hold the [`WriteLock`].
pub(super) fn save(path: &Path, keys: &[KeyRecord]) -> Result<Option<Stamp>> {
    let rendered = serde_json::to_string_pretty(&RenderedKeyFile { keys })?;
    write_text(path, &rendered, true)?;
    stamp(path)
}

/// An exclusive advisory lock beside the key file. It serializes complete
/// read/modify/write transactions across processes; `flock` also excludes
/// other open file descriptions within this process.
pub(super) struct WriteLock {
    file: fs::File,
}

impl WriteLock {
    pub(super) fn acquire(path: &Path) -> Result<Self> {
        let lock = lock_path(path)?;
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(lock)?;
        FileExt::lock_exclusive(&file)?;
        Ok(Self { file })
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        drop(FileExt::unlock(&self.file));
    }
}

fn lock_path(path: &Path) -> Result<PathBuf> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(Error::Remote(RemoteError::Failed(format!(
            "{} is not a key file path",
            path.display()
        ))));
    };
    fs::create_dir_all(parent)?;
    Ok(parent.join(format!(".{}.lock", name.to_string_lossy())))
}

#[cfg(unix)]
fn file_id(metadata: &fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(metadata)
}

#[cfg(not(unix))]
const fn file_id(_metadata: &fs::Metadata) -> u64 {
    0
}
