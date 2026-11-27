//! The entries of a repository's Hub snapshots, as a variant's view links
//! them: a snapshot entry is a hard link of its blob, as mirmir makes it,
//! or a link naming the blob, as `huggingface_hub` does. A removal takes
//! the entry itself, never a file it only names.

use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use super::blobs::{Identity, identity, identity_of};

/// The links a view's entry is followed through to find its snapshot's.
const LINKS: usize = 16;

/// The repository whose snapshots hold `path`: the `models--…` directory
/// above the `snapshots` it lies in.
pub(super) fn repository_of(path: &Path) -> Option<&Path> {
    path.ancestors().find_map(|ancestor| {
        let repository = ancestor.parent()?;
        let cached = repository.file_name()?.to_str()?.starts_with("models--");
        (cached && ancestor.file_name() == Some("snapshots".as_ref())).then_some(repository)
    })
}

/// The snapshot entry `link` names, at its path in its snapshot, the
/// folders to it canonical: `link` itself when it lies in a snapshot, else
/// the file its link names, followed until one does. `None` when it names
/// nothing in a snapshot, or nothing at all.
pub(super) fn entry(link: &Path) -> Option<PathBuf> {
    let mut path = link.to_path_buf();
    for _ in 0..LINKS {
        let folder = path.parent()?.canonicalize().ok()?;
        let named = folder.join(path.file_name()?);
        if repository_of(&folder).is_some() {
            return fs::symlink_metadata(&named).is_ok().then_some(named);
        }
        path = folder.join(fs::read_link(&named).ok()?);
    }
    None
}

/// A snapshot entry as its file system tells it apart, whatever the case
/// of the name it was reached by: its folder, its name in lower case and
/// the entry itself, a link not followed. Off Unix, its path, its name in
/// lower case on Windows.
#[cfg(unix)]
pub(super) type Key = (Identity, String, Identity);

#[cfg(not(unix))]
pub(super) type Key = PathBuf;

/// The key of the snapshot `entry`; `None` once it is gone.
#[cfg(unix)]
pub(super) fn key(entry: &Path) -> Option<Key> {
    let folder = identity(entry.parent()?)?;
    let name = entry.file_name()?.to_string_lossy().to_lowercase();
    Some((folder, name, identity_of(&fs::symlink_metadata(entry).ok()?)?))
}

#[cfg(not(unix))]
pub(super) fn key(entry: &Path) -> Option<Key> {
    let folder = entry.parent()?.canonicalize().ok()?;
    fs::symlink_metadata(entry).ok()?;
    // Windows tells no names apart by case alone.
    #[cfg(windows)]
    let name = entry.file_name()?.to_string_lossy().to_lowercase();
    #[cfg(not(windows))]
    let name = entry.file_name()?;
    Some(folder.join(name))
}
