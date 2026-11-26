use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::config::Paths;

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

/// Resolving the active selections, under the UX state lock, never takes
/// that lock again, even for a selection whose configuration still needs
/// moving, as a selection's other spelling on a case-insensitive file
/// system once did: the restoration finishes and the next operation too.
#[test]
fn resolving_active_models_never_waits_on_its_own_lock() -> Result<()> {
    let (store, root) = test_store("active-lock");
    let view = store.paths.gguf_variants_dir.join("m--Q8");
    fs::create_dir_all(&view)?;
    fs::create_dir_all(&store.paths.models_dir)?;
    fs::write(
        store.paths.models_dir.join("m--Q8.toml"),
        format!(
            "schema_version = 1\nid = \"m--Q8\"\npath = \"{}\"\n\n[hub]\nrepo_id = \"m:Q8\"\n\
             revision = \"main\"\ncommit = \"abc\"\n",
            view.canonicalize()?.display()
        ),
    )?;
    fs::write(
        &store.paths.ux_state_file,
        "schema_version = 1\nrecent_models = []\nactive_models = [\"m--Q8\"]\n",
    )?;
    let store = std::sync::Arc::new(store);
    let guard = store.ux_state_guard()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let resolving = std::sync::Arc::clone(&store);
    let worker = std::thread::spawn(move || {
        let mut state = resolving.load_ux_state().expect("state should load");
        resolving.normalize_active_models(&mut state);
        drop(sender.send(state.active_models));
    });
    let resolved = receiver.recv_timeout(std::time::Duration::from_secs(5));
    drop(guard);
    worker.join().expect("resolution should not panic");
    assert_eq!(resolved.expect("resolution should not wait on the held lock"), ["m--Q8"]);
    assert_eq!(store.active_models()?, ["m@Q8"]);
    fs::remove_dir_all(root)?;
    Ok(())
}
