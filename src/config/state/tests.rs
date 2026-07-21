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
    store.save_hub_model("Owner/Model", "main", snapshot.clone())?;
    store.activate_model(snapshot.to_str().expect("UTF-8 test path"))?;
    store.activate_model("Owner--Model")?;
    assert_eq!(store.active_models()?, ["Owner--Model"]);
    store.deactivate_model("Owner--Model")?;
    assert!(store.active_models()?.is_empty());
    Ok(())
}

#[test]
fn loads_state_written_before_active_models_existed() -> Result<()> {
    let (store, _) = test_store("state-v1");
    store.paths.ensure_config_dirs()?;
    fs::write(&store.paths.ux_state_file, "schema_version = 1\nrecent_models = ['old']\n")?;
    assert_eq!(store.recent_models()?, ["old"]);
    assert!(store.active_models()?.is_empty());
    Ok(())
}

#[test]
fn quarantines_corrupt_managed_state_and_resets_it() -> Result<()> {
    let (store, _) = test_store("corrupt-state");
    store.paths.ensure_config_dirs()?;
    fs::write(&store.paths.ux_state_file, "this is not toml = [")?;
    assert!(store.active_models()?.is_empty());
    let recovery = store.take_state_recovery()?.expect("recovery should be reported");
    assert_eq!(recovery.original, store.paths.ux_state_file);
    assert_eq!(fs::read_to_string(&recovery.quarantine)?, "this is not toml = [");
    assert!(recovery.reason.contains("TOML parse error"));
    assert!(store.take_state_recovery()?.is_none());
    assert!(store.active_models()?.is_empty());
    Ok(())
}
