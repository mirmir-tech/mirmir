use super::*;

#[test]
fn discovers_main_snapshot_from_standard_hub_layout() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "mirmir-hf-cache-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("cache")
    ));
    let repo = root.join("models--Qwen--Test");
    fs::create_dir_all(repo.join("snapshots/abc123"))?;
    fs::create_dir_all(repo.join("refs"))?;
    fs::write(repo.join("refs/main"), "abc123\n")?;

    let models = discover_in(&root, Cache::External);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].repo_id, "Qwen/Test");
    assert_eq!(models[0].revision, "main");
    assert_eq!(models[0].commit, "abc123");
    assert_eq!(models[0].repo_path, repo);
    assert_eq!(models[0].snapshot, repo.join("snapshots/abc123"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn exposes_discovery_for_a_managed_cache() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "mirmir-managed-cache-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("cache")
    ));
    let repo = root.join("models--Org--Unsupported");
    fs::create_dir_all(repo.join("snapshots/deadbeef"))?;
    fs::create_dir_all(repo.join("refs"))?;
    fs::write(repo.join("refs/main"), "deadbeef\n")?;

    let models = discover_cached_models_in(&root);
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].repo_id, "Org/Unsupported");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn discovers_incomplete_managed_downloads() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "mirmir-partial-cache-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("cache")
    ));
    let partial = root.join("models--Org--Partial/blobs/model.incomplete");
    fs::create_dir_all(partial.parent().unwrap_or(&root))?;
    fs::write(&partial, b"partial")?;

    let downloads = discover_partial_downloads_in(&root);
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].repo_id, "Org/Partial");
    assert_eq!(downloads[0].downloaded_bytes, 7);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A download interrupted between files leaves completed files in its
/// snapshot but no `.incomplete` blob; the revision reference is written
/// only after the last file, so its absence marks the download unfinished.
#[test]
fn a_managed_snapshot_without_its_reference_is_a_partial_download() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "mirmir-unreferenced-cache-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("cache")
    ));
    let repo = root.join("models--Org--Partial");
    fs::create_dir_all(repo.join("snapshots/abc123"))?;
    fs::create_dir_all(repo.join("blobs"))?;
    fs::write(repo.join("blobs/config"), b"{}")?;
    fs::write(repo.join("snapshots/abc123/config.json"), b"{}")?;

    assert!(discover_cached_models_in(&root).is_empty());
    let downloads = discover_partial_downloads_in(&root);
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].repo_id, "Org/Partial");
    // Another tool's cache keeps its own conventions.
    assert_eq!(discover_in(&root, Cache::External).len(), 1);
    fs::remove_dir_all(root)?;
    Ok(())
}
