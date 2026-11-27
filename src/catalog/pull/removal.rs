//! Removing a Hub model, or a repository with all of it: what it may
//! delete is decided once no pull or other removal of the repository can
//! run, and it deletes nothing else.

use super::{super::Removal, Activity, Catalog, download, registry::Transfer};
use crate::{
    config::{HubReference, RemovalScope},
    error::Result,
};

/// A removal ready to run: no pull or other removal of anything of its
/// repository runs until it drops, and its scope was taken under that
/// exclusion, so a caller holds off loading what it takes before it
/// deletes anything.
pub struct PreparedRemoval<'a> {
    catalog: &'a Catalog,
    scope: RemovalScope,
    _reservation: Transfer<'a>,
    _serial: tokio::sync::MutexGuard<'a, ()>,
}

impl Catalog {
    /// Prepares removing `repo_id`, a model or a whole repository in any
    /// spelling, once any other removal ends; refused while anything of
    /// its repository transfers.
    pub async fn prepare_removal(&self, repo_id: &str) -> Result<PreparedRemoval<'_>> {
        let asked = repo_id.parse::<HubReference>()?;
        let serial = self.removals.lock().await;
        let reservation = self.reserve(repo_id, Activity::Removal)?;
        Ok(PreparedRemoval {
            catalog: self,
            scope: download::removal_scope(&self.store, &asked)?,
            _reservation: reservation,
            _serial: serial,
        })
    }

    /// Removes `repo_id` as its preparation finds it.
    #[cfg(test)]
    pub async fn remove(&self, repo_id: &str) -> Result<Removal> {
        self.prepare_removal(repo_id).await?.run().await
    }
}

impl PreparedRemoval<'_> {
    /// What the removal may delete.
    #[must_use]
    pub const fn scope(&self) -> &RemovalScope {
        &self.scope
    }

    /// Removes what the scope asks for, and the repository cache its last
    /// variant leaves, deleting no cache the scope does not name.
    pub async fn run(self) -> Result<Removal> {
        let store = &self.catalog.store;
        let removed = download::remove(store, &self.scope).await?;
        let mut removal = removed.removal;
        if let Some(repository) = removed.emptied {
            // The variant is gone either way; a cache that could not go is
            // listed, and removed with its repository.
            match download::remove_emptied(store, &repository, &self.scope).await {
                Ok(freed) => removal.freed_bytes += freed,
                Err(error) => tracing::warn!(%repository, %error, "repository cache not removed"),
            }
        }
        Ok(removal)
    }
}
