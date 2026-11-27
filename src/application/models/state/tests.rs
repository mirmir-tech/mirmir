use std::path::Path;

use super::ModelLifecycle;
use crate::{
    application::Error,
    config::{CacheKind, HubReference, RemovalScope, RepositoryCache},
};

/// The removal of `asked`, deleting `caches`.
fn removal(asked: &str, caches: &[(&str, CacheKind)]) -> RemovalScope {
    let root = Path::new("/");
    let caches = caches
        .iter()
        .map(|(path, kind)| RepositoryCache::new(Path::new(path), root, *kind))
        .collect();
    RemovalScope::new(asked.parse::<HubReference>().expect("a reference"), caches)
}

#[test]
fn loading_guard_releases_the_model_on_every_exit_path() {
    let lifecycle = ModelLifecycle::default();
    let guard = lifecycle
        .begin_loading("model", Path::new("/models/model"))
        .expect("first load should start");
    assert!(matches!(
        lifecycle.begin_loading("model", Path::new("/models/model")),
        Err(Error::ModelAlreadyLoading(model)) if model == "model"
    ));

    drop(guard);

    assert!(lifecycle.begin_loading("model", Path::new("/models/model")).is_ok());
}

#[test]
fn removal_is_rejected_for_a_loading_model() {
    let lifecycle = ModelLifecycle::default();
    let _guard = lifecycle
        .begin_loading("Owner--Model@Q4_K_M", Path::new("/models/Owner--Model@Q4_K_M"))
        .expect("load should start");

    // The variant in another spelling, and its repository, are it.
    for asked in ["owner/model:q4_k_m", "owner/model"] {
        assert!(matches!(
            lifecycle.begin_removal(&removal(asked, &[])),
            Err(Error::ModelAlreadyLoading(model)) if model == "Owner--Model@Q4_K_M"
        ));
    }
    assert!(lifecycle.begin_removal(&removal("owner/model:Q8_0", &[])).is_ok());
}

/// No model of a repository being removed starts loading until the
/// removal ends.
#[test]
fn no_load_starts_during_a_removal() {
    let lifecycle = ModelLifecycle::default();
    let removal = lifecycle.begin_removal(&removal("Owner/Model", &[])).expect("removal starts");
    assert!(matches!(
        lifecycle.begin_loading("owner--model@Q8_0", Path::new("/models/owner--model@Q8_0")),
        Err(Error::ModelBeingRemoved(model)) if model == "owner--model@Q8_0"
    ));
    assert!(
        lifecycle
            .begin_loading("owner--other", Path::new("/models/owner--other"))
            .is_ok()
    );
    drop(removal);
    assert!(
        lifecycle
            .begin_loading("owner--model@Q8_0", Path::new("/models/owner--model@Q8_0"))
            .is_ok()
    );
}

/// A model loaded by its path from a repository's cache holds off a
/// removal that may delete that cache, and none that leaves it: a
/// variant's, or one of mirmir's own copy, while another tool's cache
/// holds the model loaded. No load starts from a cache being removed.
#[test]
fn a_model_loaded_from_a_cache_holds_off_only_its_removal() {
    let external = "/hf/hub/models--Owner--Repo";
    let snapshot = "/hf/hub/models--Owner--Repo/snapshots/abc";
    let own = ("/state/hf-hub/models--owner--repo", CacheKind::Managed);
    let lifecycle = ModelLifecycle::default();
    let loading = lifecycle
        .begin_loading(snapshot, Path::new(snapshot))
        .expect("load should start");
    for asked in ["owner/repo", "OWNER/REPO"] {
        let taking = removal(asked, &[(external, CacheKind::External)]);
        assert!(matches!(
            lifecycle.begin_removal(&taking),
            Err(Error::ModelAlreadyLoading(model)) if model == snapshot
        ));
    }
    for leaving in [removal("owner/repo:Q4_K_M", &[own]), removal("owner/repo", &[own])] {
        drop(lifecycle.begin_removal(&leaving).expect("the cache loaded from stays"));
    }
    drop(loading);

    let taking = removal("owner/repo", &[(external, CacheKind::External)]);
    let guard = lifecycle.begin_removal(&taking).expect("removal should start");
    assert!(matches!(
        lifecycle.begin_loading(snapshot, Path::new(snapshot)),
        Err(Error::ModelBeingRemoved(model)) if model == snapshot
    ));
    assert!(
        lifecycle
            .begin_loading(
                "/state/hf-hub/models--owner--repo2/snapshots/a",
                Path::new("/state/hf-hub/models--owner--repo2/snapshots/a")
            )
            .is_ok()
    );
    drop(guard);
    assert!(lifecycle.begin_loading(snapshot, Path::new(snapshot)).is_ok());
}

/// A configuration of its own id at a path within a cache is taken by the
/// removal that deletes that cache, whatever its key: no such removal
/// starts while it loads, and it loads during none.
#[test]
fn a_model_configured_within_a_cache_holds_off_its_removal() {
    let snapshot = Path::new("/hf/hub/models--Owner--Repo/snapshots/abc");
    let taking = removal("owner/repo", &[("/hf/hub/models--Owner--Repo", CacheKind::External)]);
    let lifecycle = ModelLifecycle::default();
    let loading = lifecycle.begin_loading("foo", snapshot).expect("load should start");
    assert!(matches!(
        lifecycle.begin_removal(&taking),
        Err(Error::ModelAlreadyLoading(model)) if model == "foo"
    ));
    drop(loading);
    let guard = lifecycle.begin_removal(&taking).expect("removal should start");
    assert!(matches!(
        lifecycle.begin_loading("foo", snapshot),
        Err(Error::ModelBeingRemoved(model)) if model == "foo"
    ));
    assert!(lifecycle.begin_loading("bar", Path::new("/models/bar")).is_ok());
    drop(guard);
}
