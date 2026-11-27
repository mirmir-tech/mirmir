//! The models being pulled or removed. A repository's variants share its
//! one Hub cache, blobs such as a vision tower included, so a removal of
//! anything of a repository excludes a pull or removal of anything else of
//! it, for as long as either runs: what a removal keeps is decided by what
//! the repository holds, which a pull changes as it goes.

use std::collections::HashSet;

use super::Catalog;
use crate::{
    config::HubReference,
    error::{Error, Result},
};

/// What a registered model is undergoing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    Pull,
    Removal,
}

impl std::fmt::Display for Activity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Pull => "is currently downloading",
            Self::Removal => "is being removed",
        })
    }
}

/// A pull or removal of `requested` refused while `held`, the same model
/// or another of its repository, undergoes `activity`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferConflict {
    pub requested: String,
    pub held: HubReference,
    pub activity: Activity,
}

impl std::fmt::Display for TransferConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { requested, held, activity } = self;
        let same = requested
            .parse::<HubReference>()
            .is_ok_and(|wanted| held.is_same_model(&wanted));
        if same {
            write!(formatter, "model `{requested}` {activity}")
        } else {
            write!(formatter, "model `{held}`, of the repository of `{requested}`, {activity}")
        }
    }
}

/// The registry's entries.
pub(in crate::catalog) type Transfers = HashSet<(HubReference, Activity)>;

/// Whether `held` keeps `wanted` from starting: the same model, or a
/// removal and anything of its repository.
fn excludes(held: &(HubReference, Activity), wanted: &(HubReference, Activity)) -> bool {
    let ((held, held_activity), (wanted, wanted_activity)) = (held, wanted);
    let removal = [held_activity, wanted_activity].contains(&&Activity::Removal);
    held.is_same_model(wanted) || (removal && held.same_repository(wanted))
}

impl Catalog {
    /// Registers `repo_id`'s `activity` until the returned guard drops.
    pub(in crate::catalog) fn reserve(
        &self,
        repo_id: &str,
        activity: Activity,
    ) -> Result<Transfer<'_>> {
        let entry = (repo_id.parse::<HubReference>()?, activity);
        let Ok(mut active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        if let Some((held, activity)) = active.iter().find(|held| excludes(held, &entry)) {
            return Err(Error::ModelTransferring(TransferConflict {
                requested: repo_id.to_owned(),
                held: held.clone(),
                activity: *activity,
            }));
        }
        active.insert(entry.clone());
        drop(active);
        Ok(Transfer { catalog: self, entry })
    }

    #[cfg(test)]
    pub(super) fn is_transferring(&self, repo_id: &str) -> Result<bool> {
        let reference = repo_id.parse::<HubReference>()?;
        let Ok(active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        Ok(active.contains(&(reference, Activity::Pull)))
    }
}

/// A registered pull or removal, forgotten when dropped, however it ends.
pub(in crate::catalog) struct Transfer<'a> {
    catalog: &'a Catalog,
    entry: (HubReference, Activity),
}

impl Drop for Transfer<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.catalog.active_transfers.lock() {
            active.remove(&self.entry);
        }
    }
}
