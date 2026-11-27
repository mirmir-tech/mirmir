//! What a removal may take: the models a reference names, by their keys,
//! and the repository caches it may delete, with every model loaded by its
//! path from within them.

use std::path::{Component, Path, PathBuf};

use super::HubReference;

/// Whose Hugging Face cache holds a repository's directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    /// mirmir's own Hub cache.
    Managed,
    /// Another tool's, which mirmir lists and loads from.
    External,
}

/// A repository's directory in a Hugging Face cache, `models--owner--name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryCache {
    path: PathBuf,
    root: PathBuf,
    kind: CacheKind,
}

impl RepositoryCache {
    /// The directory `path` of the cache `root`, both canonical, as a path
    /// a model loads by is, when they resolve.
    #[must_use]
    pub fn new(path: &Path, root: &Path, kind: CacheKind) -> Self {
        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Self {
            path: canonical(path),
            root: canonical(root),
            kind,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn kind(&self) -> CacheKind {
        self.kind
    }

    /// Whether `path`, absolute, lies within the directory, a component's
    /// case aside, as a case-insensitive file system may spell it.
    #[must_use]
    pub fn holds(&self, path: &Path) -> bool {
        path.is_absolute() && within(path, &self.path)
    }
}

/// The models a removal of `asked` takes, or holds off loading meanwhile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalScope {
    asked: HubReference,
    caches: Vec<RepositoryCache>,
}

impl RemovalScope {
    /// The removal of `asked`, which may delete `caches` and no other.
    #[must_use]
    pub const fn new(asked: HubReference, caches: Vec<RepositoryCache>) -> Self {
        Self { asked, caches }
    }

    /// The model or repository asked for.
    #[must_use]
    pub const fn asked(&self) -> &HubReference {
        &self.asked
    }

    /// The repository caches the removal may delete.
    #[must_use]
    pub fn caches(&self) -> &[RepositoryCache] {
        &self.caches
    }

    /// Whether the model keyed `key`, as the models loaded, loading and
    /// active are, is the removal's: a key `asked` owns in any spelling,
    /// or a canonical path within a cache the removal may delete.
    #[must_use]
    pub fn owns_model(&self, key: &str) -> bool {
        self.asked.owns_key(key) || self.holds_path(Path::new(key))
    }

    /// Whether the removal takes the model keyed `key` that was configured
    /// or named at `source`, canonical: by its key, or by that path, so a
    /// configuration of its own id within a cache it deletes is taken too.
    #[must_use]
    pub fn takes(&self, key: &str, source: &Path) -> bool {
        self.owns_model(key) || self.holds_path(source)
    }

    /// Whether `path`, absolute, lies within a cache the removal may
    /// delete.
    #[must_use]
    pub fn holds_path(&self, path: &Path) -> bool {
        self.caches.iter().any(|cache| cache.holds(path))
    }
}

fn within(path: &Path, directory: &Path) -> bool {
    let mut components = path.components();
    directory.components().all(|expected| {
        components.next().is_some_and(|component| match (component, expected) {
            (Component::Normal(left), Component::Normal(right)) => left.eq_ignore_ascii_case(right),
            (left, right) => left == right,
        })
    })
}

/// A removal refused: the model `model`, configured under an id of its own
/// rather than of the repository asked for, lies at `path` within a cache
/// the removal would delete.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot remove `{asked}`: model `{model}` is configured at {} within its cache; remove that \
     model first",
    path.display()
)]
pub struct HeldCache {
    asked: HubReference,
    model: String,
    path: PathBuf,
}

impl HeldCache {
    #[must_use]
    pub const fn new(asked: HubReference, model: String, path: PathBuf) -> Self {
        Self { asked, model, path }
    }
}
