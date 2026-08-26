use super::*;
use crate::config::{AppConfig, Paths, Store};

fn service(name: &str) -> (RuntimeService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "mirmir-local-{name}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("model")
    ));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    (RuntimeService::new(&AppConfig::default(), Store::new(paths)), root)
}

#[test]
fn lists_an_unconfigured_managed_snapshot_with_its_load_error() -> std::io::Result<()> {
    let (service, root) = service("unsupported");
    let snapshot = service
        .coordinator()
        .store
        .paths()
        .hub_cache_dir
        .join("models--Org--Unsupported/snapshots/a");
    std::fs::create_dir_all(&snapshot)?;
    std::fs::write(snapshot.join("config.json"), "{}")?;

    let models = service.local_models().expect("local models should be listed");
    let model = models.iter().find(|model| model.repo_id == "Org/Unsupported").unwrap();
    assert!(model.managed);
    assert_eq!(model.state, "error");
    assert!(!model.loadable);
    assert_ne!(model.load_unavailable_reason, "");
    assert_eq!(model.container, "Unknown");
    assert_eq!(model.metal_compatibility, "unsupported");
    assert_eq!(model.cuda_compatibility, "unsupported");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn lists_a_partial_download_as_paused() -> std::io::Result<()> {
    let (service, root) = service("partial");
    let partial = service
        .coordinator()
        .store
        .paths()
        .hub_cache_dir
        .join("models--Org--Partial/blobs/a.incomplete");
    std::fs::create_dir_all(partial.parent().unwrap())?;
    std::fs::write(partial, b"partial")?;

    let models = service.local_models().expect("local models should be listed");
    let model = models.iter().find(|model| model.repo_id == "Org/Partial").unwrap();
    assert_eq!(model.state, "paused");
    assert_eq!(model.size_bytes, 7);
    assert!(!model.loadable);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
