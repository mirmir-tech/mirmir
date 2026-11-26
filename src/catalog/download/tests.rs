use super::{
    remove::{remove_cache_repo, validated_cache_repo},
    *,
};

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "mirmir-{name}-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

#[test]
fn accepts_only_direct_model_repositories_in_cache() -> Result<()> {
    let root = root("remove-cache");
    let cache = root.join("cache");
    let model = cache.join("models--Qwen--Test");
    let nested = cache.join("nested/models--Qwen--Nested");
    let outside = root.join("models--Qwen--Outside");
    std::fs::create_dir_all(&model)?;
    std::fs::create_dir_all(&nested)?;
    std::fs::create_dir_all(&outside)?;
    assert_eq!(validated_cache_repo(&model, &cache)?, model.canonicalize()?);
    assert!(validated_cache_repo(&nested, &cache).is_err());
    assert!(validated_cache_repo(&outside, &cache).is_err());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn removes_verified_external_cache_repository() -> Result<()> {
    let root = root("remove-external");
    let cache = root.join("hub");
    let model = cache.join("models--Qwen--External");
    let weights = model.join("snapshots/abc/model.safetensors");
    std::fs::create_dir_all(weights.parent().unwrap_or(&model))?;
    std::fs::write(&weights, b"weights")?;

    let freed = remove_cache_repo(model.clone(), cache).await?;
    assert_eq!(freed, 7);
    assert!(!model.exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn removes_the_managed_partial_repository() -> Result<()> {
    let root = root("discard-partial");
    let paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    let repo = paths.hub_cache_dir.join("models--Qwen--Partial");
    let incomplete = repo.join("blobs/weights.incomplete");
    std::fs::create_dir_all(incomplete.parent().unwrap_or(&repo))?;
    std::fs::write(&incomplete, b"partial")?;
    let removal = remove(&Store::new(paths), "Qwen/Partial").await?.removal;
    assert!(removal.removed);
    assert_eq!(removal.freed_bytes, 7);
    assert!(!repo.exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// Removing a variant the repository never had keeps its base model: the
/// configuration and the weight bytes.
#[tokio::test]
async fn removing_an_absent_variant_keeps_the_base_model() -> Result<()> {
    let root = root("absent-variant");
    let paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    let snapshot = paths.hub_cache_dir.join("models--owner--review/snapshots/abc");
    std::fs::create_dir_all(&snapshot)?;
    std::fs::write(snapshot.join("config.json"), b"{}")?;
    std::fs::write(snapshot.join("model.safetensors"), b"weights")?;
    let store = Store::new(paths);
    store.save_hub_model("owner/review", "main", "abc", snapshot.clone())?;
    let removal = remove(&store, "owner/review:Q8_0").await?.removal;
    assert!(!removal.removed);
    assert_eq!(removal.freed_bytes, 0);
    assert_eq!(std::fs::read(snapshot.join("model.safetensors"))?, b"weights");
    assert!(snapshot.join("config.json").exists());
    let saved = store.list_model_configs()?;
    assert!(
        saved
            .iter()
            .any(|model| model.hub.as_ref().is_some_and(|hub| hub.repo_id == "owner/review"))
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
