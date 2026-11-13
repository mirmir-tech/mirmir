use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::config::{Paths, Store};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct LinkedHub {
    root: PathBuf,
    linked_snapshot: PathBuf,
    store: Store,
}

impl LinkedHub {
    /// Mirrors `$MIRMIR_STATE_HOME/hf-hub -> ~/.local/state/mirmir/hf-hub`.
    fn new() -> std::io::Result<Self> {
        let id = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("mirmir-linked-hub-{}-{id}", std::process::id()));
        let real = root.join("real/hf-hub");
        fs::create_dir_all(real.join("models--Owner--Model/snapshots/commit"))?;
        fs::create_dir_all(root.join("state"))?;
        std::os::unix::fs::symlink(&real, root.join("state/hf-hub"))?;
        let paths =
            Paths::from_roots(root.join("config"), root.join("state"), &root.join("runtime"));
        let linked_snapshot = paths.hub_cache_dir.join("models--Owner--Model/snapshots/commit");
        Ok(Self {
            root,
            linked_snapshot,
            store: Store::new(paths),
        })
    }

    fn cached(&self) -> CachedModel {
        CachedModel {
            repo_id: "Owner/Model".to_owned(),
            revision: "main".to_owned(),
            commit: "commit".to_owned(),
            repo_path: self.store.paths().hub_cache_dir.join("models--Owner--Model"),
            snapshot: self.linked_snapshot.clone(),
        }
    }
}

impl Drop for LinkedHub {
    fn drop(&mut self) {
        let _removed = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn cached_model_residency_matches_the_canonical_active_selector() -> crate::error::Result<()> {
    let hub = LinkedHub::new()?;
    let active = hub.store.resolve_model(hub.linked_snapshot.to_str().expect("UTF-8"))?.key;
    assert_ne!(Path::new(&active), hub.linked_snapshot, "fixture must traverse a symlink");
    let loading = HashSet::from([active.clone()]);
    let mut listed = Vec::new();
    append_cached(
        &mut listed,
        hub.cached(),
        true,
        &mut HashSet::new(),
        &mut HashSet::new(),
        &HashMap::new(),
        &loading,
    );
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].selector, active);
    assert_eq!(listed[0].path, active);
    assert_eq!(listed[0].state, LocalModelState::Loading);
    Ok(())
}

#[test]
fn cached_model_already_configured_through_a_symlink_is_not_listed_twice() -> std::io::Result<()> {
    let hub = LinkedHub::new()?;
    let mut known_paths = HashSet::from([canonical(&hub.linked_snapshot)]);
    let mut listed = Vec::new();
    append_cached(
        &mut listed,
        hub.cached(),
        true,
        &mut HashSet::new(),
        &mut known_paths,
        &HashMap::new(),
        &HashSet::new(),
    );
    assert!(listed.is_empty());
    Ok(())
}
