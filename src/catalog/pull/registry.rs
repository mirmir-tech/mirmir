//! The models being pulled or removed. A repository's variants share its
//! one Hub cache, so removing a repository excludes a transfer of any of
//! its variants, and a pull that may choose any variant excludes removing
//! one, for as long as either runs.

use std::collections::HashSet;

use super::Catalog;
use crate::{
    config::HubReference,
    error::{Error, Result},
};

/// What a registered model is undergoing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::catalog) enum Activity {
    Pull,
    Removal,
}

/// The registry's entries.
pub(in crate::catalog) type Transfers = HashSet<(HubReference, Activity)>;

/// Whether `held` keeps `wanted` from starting: the same model, or a
/// removal and anything else of its repository where either names the
/// whole repository.
fn excludes(held: &(HubReference, Activity), wanted: &(HubReference, Activity)) -> bool {
    let ((held, held_activity), (wanted, wanted_activity)) = (held, wanted);
    let removal = [held_activity, wanted_activity].contains(&&Activity::Removal);
    held == wanted
        || (removal
            && held.repository() == wanted.repository()
            && (held.variant().is_none() || wanted.variant().is_none()))
}

impl Catalog {
    /// Registers `repo_id`'s `activity` until the returned guard drops.
    pub(super) fn reserve(&self, repo_id: &str, activity: Activity) -> Result<Transfer<'_>> {
        let entry = (repo_id.parse::<HubReference>()?, activity);
        let Ok(mut active) = self.active_transfers.lock() else {
            return Err(Error::Config("model transfer registry lock is poisoned".to_owned()));
        };
        if let Some((_, held)) = active.iter().find(|held| excludes(held, &entry)) {
            return Err(Error::Config(match held {
                Activity::Pull => format!("model `{repo_id}` is currently downloading"),
                Activity::Removal => format!("model `{repo_id}` is being removed"),
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
pub(super) struct Transfer<'a> {
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
