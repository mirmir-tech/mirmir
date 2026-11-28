use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::config::{CacheKind, Paths, RemovalScope, RepositoryCache};

static NEXT_STATE: AtomicU64 = AtomicU64::new(0);

fn test_store(name: &str) -> (Store, std::path::PathBuf) {
    let id = NEXT_STATE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-{name}-{}-{id}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("runtime"));
    (Store::new(paths), root)
}

#[test]
fn remembers_models_in_most_recent_order() -> Result<()> {
    let (store, _) = test_store("state");
    store.remember_model("first")?;
    store.remember_model("second")?;
    store.remember_model("first")?;
    assert_eq!(store.recent_models()?, ["first", "second"]);
    Ok(())
}

#[test]
fn persists_the_active_model_set() -> Result<()> {
    let (store, _) = test_store("active");
    store.activate_model("first")?;
    store.activate_model("second")?;
    store.activate_model("first")?;
    assert_eq!(store.active_models()?, ["first", "second"]);
    store.deactivate_model("first")?;
    assert_eq!(store.active_models()?, ["second"]);
    Ok(())
}

/// A removed model leaves the active set in whichever spelling it was
/// activated, and a removed repository takes each of its variants along.
#[test]
fn a_removal_deactivates_every_spelling_it_names() -> Result<()> {
    let (store, _) = test_store("active-removed");
    for key in ["Org--Repo@Q4_K_M", "Org--Repo@Q8_0", "Org--Repo-GGUF@Q8_0", "Org--Other"] {
        store.activate_model(key)?;
    }
    store.deactivate_models_of(&RemovalScope::new("org/repo:q4_k_m".parse()?, Vec::new()))?;
    assert_eq!(store.active_models()?, ["Org--Repo@Q8_0", "Org--Repo-GGUF@Q8_0", "Org--Other"]);
    store.deactivate_models_of(&RemovalScope::new("org/repo".parse()?, Vec::new()))?;
    assert_eq!(store.active_models()?, ["Org--Repo-GGUF@Q8_0", "Org--Other"]);
    Ok(())
}

/// A model activated by its path in a repository's Hugging Face cache,
/// unconfigured, leaves the active set with a removal that deleted that
/// cache; one whose files stay, as a variant's removal leaves another
/// tool's cache, or in a cache the removal leaves, stays active.
#[test]
fn a_removal_deactivates_a_model_activated_from_a_cache_it_took() -> Result<()> {
    let (store, root) = test_store("active-cached");
    let external = root.join("hf/models--Owner--Repo");
    let namesake = root.join("hf/models--Owner--Repo2/snapshots/abc");
    let mut activated = Vec::new();
    for snapshot in [&external.join("snapshots/abc"), &namesake] {
        fs::create_dir_all(snapshot)?;
        let key = fs::canonicalize(snapshot)?.display().to_string();
        store.activate_model(&key)?;
        activated.push(key);
    }
    let cache = RepositoryCache::new(&external, &root.join("hf"), CacheKind::External);
    // Its variant's removal takes mirmir's own cache, not another tool's.
    store.deactivate_models_of(&RemovalScope::new("owner/repo:Q4_K_M".parse()?, Vec::new()))?;
    // A removal that may take the cache but has not leaves the model.
    let repository = RemovalScope::new("owner/repo".parse()?, vec![cache]);
    store.deactivate_models_of(&repository)?;
    assert_eq!(store.active_models()?, activated);
    fs::remove_dir_all(&external)?;
    store.deactivate_models_of(&repository)?;
    assert_eq!(store.active_models()?, activated[1..]);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn canonicalizes_active_model_paths_and_removes_aliases() -> Result<()> {
    let (store, root) = test_store("active-alias");
    let snapshot = root.join("hub/snapshots/commit");
    fs::create_dir_all(&snapshot)?;
    store.save_hub_model("Owner/Model", "main", "commit", snapshot.clone())?;
    store.activate_model(snapshot.to_str().expect("UTF-8 test path"))?;
    store.activate_model("Owner--Model")?;
    assert_eq!(store.active_models()?, ["Owner--Model"]);
    store.deactivate_model("Owner--Model")?;
    assert_eq!(store.active_models()?, Vec::<String>::new());
    Ok(())
}

#[test]
fn loads_state_written_before_active_models_existed() -> Result<()> {
    let (store, _) = test_store("state-v1");
    store.paths.ensure_config_dirs()?;
    fs::write(&store.paths.ux_state_file, "schema_version = 1\nrecent_models = ['old']\n")?;
    assert_eq!(store.recent_models()?, ["old"]);
    assert_eq!(store.active_models()?, Vec::<String>::new());
    Ok(())
}

#[test]
fn quarantines_corrupt_managed_state_and_resets_it() -> Result<()> {
    let (store, _) = test_store("corrupt-state");
    store.paths.ensure_config_dirs()?;
    fs::write(&store.paths.ux_state_file, "this is not toml = [")?;
    assert_eq!(store.active_models()?, Vec::<String>::new());
    let recovery = store.take_state_recovery()?.expect("recovery should be reported");
    assert_eq!(recovery.original, store.paths.ux_state_file);
    assert_eq!(fs::read_to_string(&recovery.quarantine)?, "this is not toml = [");
    assert!(recovery.reason.contains("TOML parse error"));
    assert!(store.take_state_recovery()?.is_none());
    assert_eq!(store.active_models()?, Vec::<String>::new());
    Ok(())
}

/// The reviewer's case on a case-insensitive file system (macOS APFS): an
/// installed `owner/model-GGUF:Q4_0` and an active selection spelled
/// `owner/model-GGUF:q4_0`, whose key names the same configuration file
/// there. Listing the active models finishes, the next state operation too,
/// and the configuration stays. Elsewhere the spellings are two names, and
/// the same calls hold.
#[test]
fn an_active_selection_spelled_another_way_resolves_without_waiting() -> Result<()> {
    let (store, root) = test_store("active-spelling");
    let snapshot = store.paths.hub_cache_dir.join("models--owner--model-GGUF/snapshots/abc");
    fs::create_dir_all(&snapshot)?;
    let file = snapshot.join("m-Q4_0.gguf");
    fs::write(&file, b"GGUF")?;
    store.save_hub_variant(
        "owner/model-GGUF:Q4_0",
        "main",
        "abc",
        file.clone(),
        vec!["m-Q4_0.gguf".to_owned()],
        None,
    )?;
    let saved = store.paths.models_dir.join("owner--model-GGUF@Q4_0.toml");
    fs::write(
        &store.paths.ux_state_file,
        "schema_version = 1\nrecent_models = []\nactive_models = [\"owner/model-GGUF:q4_0\"]\n",
    )?;
    let store = std::sync::Arc::new(store);
    let (sender, receiver) = std::sync::mpsc::channel();
    let listing = std::sync::Arc::clone(&store);
    let worker = std::thread::spawn(move || {
        let active = listing.active_models();
        let next = listing.recent_models();
        sender.send((active.is_ok(), next.is_ok())).ok();
    });
    let finished = receiver.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(finished.ok(), Some((true, true)), "listing or the next operation waited");
    worker.join().expect("listing should not panic");
    assert!(saved.exists(), "the installed variant's configuration went");
    assert!(store.hub_model_downloaded("owner/model-GGUF:Q4_0")?);
    // Restored as the installed variant, which loads directly from its snapshot
    // file.
    let active = store.active_models()?;
    assert_eq!(active, ["owner--model-GGUF@Q4_0"]);
    assert_eq!(store.resolve_model(&active[0])?.path, file);
    fs::remove_dir_all(root)?;
    Ok(())
}
