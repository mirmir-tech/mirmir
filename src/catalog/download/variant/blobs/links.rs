//! The links in a repository's snapshots, by the blob each names: those
//! of every commit, configured or not, such as another revision's or a
//! snapshot mirmir lists without a configuration. A blob one of them
//! names stays, or the link would name nothing.

use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};

use super::super::snapshot::{self, Key};

/// The snapshot links naming each blob, by its canonical path, and the
/// blob each link names.
#[derive(Default)]
pub(super) struct Links {
    naming: HashMap<PathBuf, HashSet<Key>>,
    named: HashMap<Key, PathBuf>,
}

impl Links {
    /// The links under the `snapshots` of `repository`, canonical, naming
    /// a blob of its own. A folder that cannot be listed is an error; a
    /// link naming nothing, or no blob, names none.
    pub fn walk(repository: &Path) -> io::Result<Self> {
        #[cfg(all(test, unix))]
        super::identity::count((0, 0, 1));
        let mut links = Self::default();
        let Some(blobs) = super::blobs_of(repository)? else {
            return Ok(links);
        };
        let mut folders = vec![repository.join("snapshots")];
        while let Some(folder) = folders.pop() {
            let entries = match fs::read_dir(&folder) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                entries => entries?,
            };
            for entry in entries {
                let (entry, kind) =
                    entry.and_then(|entry| Ok((entry.path(), entry.file_type()?)))?;
                if kind.is_dir() {
                    folders.push(entry);
                } else if kind.is_symlink()
                    && let Ok(blob) = entry.canonicalize()
                    && blob.parent() == Some(blobs.as_path())
                    && let Some(key) = snapshot::key(&entry)
                {
                    links.naming.entry(blob.clone()).or_default().insert(key.clone());
                    links.named.insert(key, blob);
                }
            }
        }
        Ok(links)
    }

    /// Whether a link besides `entry` names `blob`.
    pub fn name(&self, blob: &Path, entry: Option<&Key>) -> bool {
        let mut naming = self.naming.get(blob).into_iter().flatten();
        naming.any(|link| Some(link) != entry)
    }

    /// Forgets the link `entry`, removed.
    pub fn forget(&mut self, entry: &Key) {
        let blob = self.named.remove(entry);
        if let Some(naming) = blob.and_then(|blob| self.naming.get_mut(&blob)) {
            naming.remove(entry);
        }
    }
}
