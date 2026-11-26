//! Removing files as the bytes their removal frees. A file's bytes go with
//! its last link: a cache blob hard-linked into a snapshot frees them once,
//! with whichever of the two goes last, and a symbolic link frees none.
//! Where links cannot be counted (off Unix) no bytes are reported freed.

use std::{fs, io, path::Path};

/// Removes the file or link `path`; the bytes freed.
pub(in crate::catalog::download) fn remove_file(path: &Path) -> io::Result<u64> {
    let freed = fs::symlink_metadata(path).map_or(0, |metadata| last_link_bytes(&metadata));
    fs::remove_file(path)?;
    Ok(freed)
}

/// Removes the directory `path` with everything in it, following no link;
/// the bytes freed. An entry gone meanwhile frees nothing more.
pub(in crate::catalog::download) fn remove_tree(path: &Path) -> io::Result<u64> {
    let mut freed = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let removed = if kind.is_dir() {
            remove_tree(&entry.path())
        } else if is_directory_link(kind) {
            fs::remove_dir(entry.path()).map(|()| 0)
        } else {
            remove_file(&entry.path())
        };
        freed += match removed {
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            removed => removed?,
        };
    }
    fs::remove_dir(path)?;
    Ok(freed)
}

/// The bytes of a file the removal of its link `metadata` describes frees:
/// none while another link keeps it, or when its links cannot be counted.
fn last_link_bytes(metadata: &fs::Metadata) -> u64 {
    if !metadata.file_type().is_file() || !last_link(metadata) {
        return 0;
    }
    metadata.len()
}

#[cfg(unix)]
fn last_link(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() == 1
}

/// Without link counts a snapshot entry cannot be told from the blob it
/// links: its bytes would count once for each, or for a blob that stays.
/// What a removal frees there is reported as nothing, never as more.
#[cfg(not(unix))]
const fn last_link(_metadata: &fs::Metadata) -> bool {
    false
}

/// Whether `kind` is a link to a directory, which Windows removes as one.
#[cfg(windows)]
fn is_directory_link(kind: fs::FileType) -> bool {
    use std::os::windows::fs::FileTypeExt;
    kind.is_symlink_dir()
}

#[cfg(not(windows))]
const fn is_directory_link(_kind: fs::FileType) -> bool {
    false
}
