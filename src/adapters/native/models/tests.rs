use super::super::NativeRuntime;
use crate::{
    application::Error,
    config::{AppConfig, HubReference, Paths, RemovalScope, Store},
};

/// No load of a model being removed starts while its removal is in flight.
#[test]
fn no_load_resolves_a_model_being_removed() -> crate::error::Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-load-removing-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let snapshot = paths.hub_cache_dir.join("models--owner--m-GGUF/snapshots/abc");
    std::fs::create_dir_all(&snapshot)?;
    let file = snapshot.join("m-Q4_0.gguf");
    std::fs::write(&file, b"GGUF")?;
    let store = Store::new(paths);
    store.save_hub_variant(
        "owner/m-GGUF:Q4_0",
        "main",
        "abc",
        file,
        vec!["m-Q4_0.gguf".to_owned()],
        None,
    )?;
    let runtime = NativeRuntime::new(&AppConfig::default(), store);
    let asked = RemovalScope::new("Owner/M-gguf".parse::<HubReference>()?, Vec::new());
    let removal = runtime.lifecycle.begin_removal(&asked);
    assert!(removal.is_ok());
    let refused = runtime.load_model("owner/m-GGUF:Q4_0", false, &mut |_| {});
    assert!(matches!(refused, Err(Error::ModelBeingRemoved(_))));
    drop(removal);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
