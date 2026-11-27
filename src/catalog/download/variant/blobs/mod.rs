//! The blobs of a repository's Hub cache, found from the snapshot entries
//! that name them: a link names its blob, a hard link is one of it, the
//! file of its device and inode in the repository's `blobs`. A removal
//! lists a repository's `blobs` and walks its snapshots' links once, and
//! finds the blob of every shard and view it removes, and the links still
//! naming it, in what it read.
//!
//! No pull or other removal of the repository in this process adds a blob
//! or a link meanwhile; another process may, such as `huggingface-cli` or
//! another mirmir sharing the cache. A listed blob is read again before it
//! is returned, and a hard link changed since the listing has `blobs`
//! listed again, so a blob is never mistaken for another; one linked
//! within the file system's timestamp granule of the listing may be
//! missed, and then stays. A link another process makes meanwhile is not
//! seen.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(all(test, unix))]
pub(super) use identity::READS;
pub(super) use identity::{Identity, identity, identity_of};
use identity::{changed_after, metadata};
use links::Links;

use super::{super::freed, snapshot};

mod identity;
mod links;

/// The blob a snapshot entry names, how it names it, and the blob's
/// identity where files have one.
pub(super) struct Blob {
    pub repository: PathBuf,
    pub path: PathBuf,
    pub naming: Naming,
    pub identity: Option<Identity>,
}

/// How a snapshot entry names its blob, which decides which goes first: a
/// hard link's blob, the entry naming it still should its removal fail;
/// a link, never left naming a blob that is gone.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Naming {
    HardLink,
    Link,
}

/// What a removal read of each repository it found a blob in.
#[derive(Default)]
pub(in crate::catalog::download) struct Blobs {
    repositories: HashMap<PathBuf, Repository>,
}

/// A repository's `blobs`, listed on the first lookup of a hard link, and
/// its snapshots' links, walked on the first question of what names one.
#[derive(Default)]
struct Repository {
    listing: Option<Listing>,
    links: Option<Links>,
}

/// A repository's `blobs`, by identity, as they were listed `at`. A blob
/// of an identity is the first one listed, as a scan would find.
struct Listing {
    blobs: HashMap<Identity, PathBuf>,
    at: SystemTime,
}

/// What a listing holds of an identity: a blob still of it, a blob since
/// gone or replaced, or none as of when it was listed.
enum Listed {
    Found(PathBuf),
    Changed,
    Absent(SystemTime),
}

impl Blobs {
    /// The blob the snapshot `entry` names: its link's target in the
    /// repository's `blobs`, or the blob its hard link is. A listing that
    /// fails is an error, so that no entry goes without its blob.
    pub(super) fn of(&mut self, entry: &Path) -> io::Result<Option<Blob>> {
        let Some(repository) = snapshot::repository_of(entry) else {
            return Ok(None);
        };
        let file = match fs::symlink_metadata(entry) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            file => file?,
        };
        if file.file_type().is_symlink() {
            return linked(entry, repository);
        }
        let Some(identity) = identity_of(&file) else {
            return Ok(None);
        };
        let known = self.repositories.entry(repository.to_path_buf()).or_default();
        let path = match known.listing.as_ref().map(|listing| listing.find(identity)) {
            Some(Listed::Found(path)) => Some(path),
            Some(Listed::Absent(at)) if !changed_after(&file, at) => None,
            // Unlisted, changed, or linked since it was listed.
            _ => match known.list(repository)?.find(identity) {
                Listed::Found(path) => Some(path),
                Listed::Changed | Listed::Absent(_) => None,
            },
        };
        let naming = Naming::HardLink;
        let repository = repository.to_path_buf();
        Ok(path.map(|path| Blob {
            repository,
            path,
            naming,
            identity: Some(identity),
        }))
    }

    /// Whether a link in the snapshots of `repository` besides `entry`
    /// names its canonical `blob`.
    pub(super) fn named(
        &mut self,
        repository: &Path,
        blob: &Path,
        entry: Option<&snapshot::Key>,
    ) -> io::Result<bool> {
        let known = self.repositories.entry(repository.to_path_buf()).or_default();
        let links = match &mut known.links {
            Some(links) => links,
            links => links.insert(Links::walk(repository)?),
        };
        Ok(links.name(blob, entry))
    }

    /// Removes the blob `path`, which no later lookup finds; the bytes freed.
    pub(super) fn remove(&mut self, path: &Path) -> io::Result<u64> {
        let identity = identity(path);
        let freed = freed::remove_file(path)?;
        if let Some(identity) = identity {
            let listings =
                self.repositories.values_mut().filter_map(|known| known.listing.as_mut());
            for listing in listings {
                let listed = listing.blobs.get(&identity);
                if listed.is_some_and(|listed| listed.file_name() == path.file_name()) {
                    listing.blobs.remove(&identity);
                }
            }
        }
        Ok(freed)
    }

    /// Removes the snapshot `entry`, keyed `key`, which then names no
    /// blob; the bytes freed.
    pub(super) fn remove_entry(
        &mut self,
        entry: &Path,
        key: Option<&snapshot::Key>,
    ) -> io::Result<u64> {
        let freed = freed::remove_file(entry)?;
        if let Some(key) = key {
            let walked = self.repositories.values_mut().filter_map(|known| known.links.as_mut());
            walked.for_each(|links| links.forget(key));
        }
        Ok(freed)
    }
}

#[cfg(test)]
impl Blobs {
    /// Takes the listings as made `by` earlier, past any timestamp granule.
    pub(super) fn list_earlier(&mut self, by: std::time::Duration) {
        let listings = self.repositories.values_mut().filter_map(|known| known.listing.as_mut());
        listings.for_each(|listing| listing.at -= by);
    }
}

impl Repository {
    /// Lists the `blobs` of `repository`, in place of an earlier listing;
    /// one that fails leaves none. What is not a file there is no blob.
    fn list(&mut self, repository: &Path) -> io::Result<&Listing> {
        #[cfg(all(test, unix))]
        identity::count((0, 1, 0));
        self.listing = None;
        let at = SystemTime::now();
        let mut blobs = HashMap::new();
        let entries = blobs_of(repository)?.map(fs::read_dir).transpose()?;
        for entry in entries.into_iter().flatten() {
            let path = entry?.path();
            let blob = match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                blob => blob?,
            };
            if let Some(identity) = identity_of(&blob).filter(|_| blob.is_file()) {
                blobs.entry(identity).or_insert(path);
            }
        }
        Ok(self.listing.insert(Listing { blobs, at }))
    }
}

impl Listing {
    fn find(&self, identity: Identity) -> Listed {
        match self.blobs.get(&identity) {
            Some(path) if self::identity(path) == Some(identity) => Listed::Found(path.clone()),
            Some(_) => Listed::Changed,
            None => Listed::Absent(self.at),
        }
    }
}

/// The `blobs` of `repository`, canonical, which may be a link to a
/// folder elsewhere; `None` for a repository without.
pub(super) fn blobs_of(repository: &Path) -> io::Result<Option<PathBuf>> {
    match repository.join("blobs").canonicalize() {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        blobs => blobs.map(Some),
    }
}

/// The blob the link `entry` names, when it is one of `repository`'s.
fn linked(entry: &Path, repository: &Path) -> io::Result<Option<Blob>> {
    let (Some(blobs), Ok(path)) = (blobs_of(repository)?, entry.canonicalize()) else {
        return Ok(None);
    };
    let identity = metadata(&path).ok().as_ref().and_then(identity_of);
    let (repository, naming) = (repository.to_path_buf(), Naming::Link);
    Ok((path.parent() == Some(blobs.as_path())).then_some(Blob {
        repository,
        path,
        naming,
        identity,
    }))
}
