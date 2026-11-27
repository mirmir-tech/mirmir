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
        &mut super::known::Repositories::default(),
        &mut HashSet::new(),
        &HashMap::new(),
        &loading,
        &hub.store,
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
        &mut super::known::Repositories::default(),
        &mut known_paths,
        &HashMap::new(),
        &HashSet::new(),
        &hub.store,
    );
    assert!(listed.is_empty());
    Ok(())
}

#[test]
fn resident_cached_vision_model_reports_its_image_input() -> crate::error::Result<()> {
    let hub = LinkedHub::new()?;
    let active = hub.store.resolve_model(hub.linked_snapshot.to_str().expect("UTF-8"))?.key;
    assert_ne!(Path::new(&active), hub.linked_snapshot, "fixture must traverse a symlink");
    let loaded = ModelInfo {
        id: active.clone(),
        path: active.clone(),
        image_input: true,
        image_unavailable_reason: String::new(),
    };
    let resident = HashMap::from([(active.as_str(), &loaded)]);
    let mut listed = Vec::new();
    append_cached(
        &mut listed,
        hub.cached(),
        false,
        &mut super::known::Repositories::default(),
        &mut HashSet::new(),
        &resident,
        &HashSet::new(),
        &hub.store,
    );
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].selector, active);
    assert_eq!(listed[0].state, LocalModelState::Ready);
    assert!(listed[0].image_input, "a resident vision model must accept images");
    Ok(())
}

#[test]
fn resident_cached_text_model_keeps_its_image_unavailable_reason() -> crate::error::Result<()> {
    let hub = LinkedHub::new()?;
    let active = hub.store.resolve_model(hub.linked_snapshot.to_str().expect("UTF-8"))?.key;
    let loaded = ModelInfo {
        id: active.clone(),
        path: active.clone(),
        image_input: false,
        image_unavailable_reason: "text-only checkpoint".to_owned(),
    };
    let resident = HashMap::from([(active.as_str(), &loaded)]);
    let mut listed = Vec::new();
    append_cached(
        &mut listed,
        hub.cached(),
        true,
        &mut super::known::Repositories::default(),
        &mut HashSet::new(),
        &resident,
        &HashSet::new(),
        &hub.store,
    );
    assert_eq!(listed[0].state, LocalModelState::Ready);
    assert!(!listed[0].image_input);
    assert_eq!(listed[0].image_unavailable_reason, "text-only checkpoint");
    Ok(())
}

/// A repository's partial cache lists as the GGUF variant downloading into
/// it, whose resume pulls that variant, not as the whole repository.
#[test]
fn a_partial_variant_download_lists_as_the_variant() -> std::io::Result<()> {
    let hub = LinkedHub::new()?;
    let paths = hub.store.paths();
    let blobs = paths.hub_cache_dir.join("models--Owner--Model-GGUF/blobs");
    fs::create_dir_all(&blobs)?;
    fs::write(blobs.join("etag.incomplete"), [0_u8; 7])?;
    let listed = |known: &mut super::known::Repositories| {
        let mut listed = Vec::new();
        partial::append(&mut listed, paths, crate::catalog::pending_variants(paths), known);
        listed
            .into_iter()
            .filter(|model| model.repo_id.starts_with("Owner/Model-GGUF"))
            .map(|model| (model.repo_id, model.size_bytes))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        listed(&mut super::known::Repositories::default()),
        [("Owner/Model-GGUF".to_owned(), 7)]
    );
    fs::create_dir_all(&paths.gguf_variants_dir)?;
    fs::write(
        paths.gguf_variants_dir.join("Owner--Model-GGUF@Q4_K_M.pending"),
        "Owner/Model-GGUF:Q4_K_M",
    )?;
    assert_eq!(
        listed(&mut super::known::Repositories::default()),
        [("Owner/Model-GGUF:Q4_K_M".to_owned(), 7)]
    );
    // Its last bytes are in, its configuration not yet saved: still the
    // variant, and the repository's cache is no model of its own.
    fs::rename(blobs.join("etag.incomplete"), blobs.join("etag"))?;
    let mut known = super::known::Repositories::default();
    assert_eq!(listed(&mut known), [("Owner/Model-GGUF:Q4_K_M".to_owned(), 7)]);
    assert!(known.contains("Owner/Model-GGUF"));
    // A repository known in another spelling lists no row of its own.
    let mut other = super::known::Repositories::default();
    other.insert("owner/model-gguf:q4_k_m");
    assert!(listed(&mut other).is_empty());
    Ok(())
}

/// A model whose files are gone is missing, not failing to load; one on
/// disk that does not load is an error, a state that still counts as
/// downloaded.
#[test]
fn gone_files_are_missing_and_unloadable_ones_an_error() -> std::io::Result<()> {
    let hub = LinkedHub::new()?;
    let (resident, loading) = (HashMap::new(), HashSet::new());
    let gone = hub.root.join("gone");
    assert_eq!(state("gone", &gone, false, &resident, &loading), LocalModelState::Missing);
    let present = &hub.linked_snapshot;
    assert_eq!(state("bad", present, false, &resident, &loading), LocalModelState::Error);
    assert_eq!(state("good", present, true, &resident, &loading), LocalModelState::Available);
    Ok(())
}
