use super::super::NativeRuntime;
use crate::{
    application::Error,
    config::{AppConfig, HubReference, Paths, Store},
};

/// No load of a model being removed starts, nor makes its GGUF import,
/// which the removal would leave behind or race.
#[test]
fn no_load_resolves_a_model_being_removed() -> crate::error::Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-load-removing-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let view = paths.gguf_variants_dir.join("owner--m-GGUF@Q4_0");
    std::fs::create_dir_all(&view)?;
    std::fs::write(view.join("m-Q4_0.gguf"), b"GGUF")?;
    let store = Store::new(paths);
    store.save_hub_model("owner/m-GGUF:Q4_0", "main", "abc", view.canonicalize()?)?;
    let import = store.gguf_import_dir(&view.canonicalize()?);
    let runtime = NativeRuntime::new(&AppConfig::default(), store);
    let asked = "Owner/M-gguf".parse::<HubReference>()?;
    let removal = runtime.lifecycle.begin_removal(&asked);
    assert!(removal.is_ok());
    let refused = runtime.load_model("owner/m-GGUF:Q4_0", false, &mut |_| {});
    assert!(matches!(refused, Err(Error::ModelBeingRemoved(_))));
    assert!(!import.exists());
    drop(removal);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
