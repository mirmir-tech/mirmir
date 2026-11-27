use super::{
    remove::{remove_cache_repo, validated_cache_repo},
    *,
};
use crate::config::{CacheKind, RepositoryCache};

/// Removes `repo_id` within the scope it has now.
async fn removing(store: &Store, repo_id: &str) -> Result<Removal> {
    let scope = removal_scope(store, &repo_id.parse()?)?;
    Ok(remove(store, &scope).await?.removal)
}

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

    let paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    let external = RepositoryCache::new(&model, &cache, CacheKind::External);
    let freed = remove_cache_repo(&Store::new(paths), external).await?;
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
    let removal = removing(&Store::new(paths), "Qwen/Partial").await?;
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
    let removal = removing(&store, "owner/review:Q8_0").await?;
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

/// A repository's cache goes with the GGUF imports that listing or loading
/// a model by a path in it made: its snapshot's, a file's linked into it,
/// by the blob that path resolves to, and a file's of its own, current or
/// legacy.
#[cfg(unix)]
#[tokio::test]
async fn removing_a_cache_takes_its_imports() -> Result<()> {
    let root = root("remove-imports");
    let paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    let imports_dir = paths.gguf_imports_dir.clone();
    let repo = paths.hub_cache_dir.join("models--Owner--Gguf");
    let (snapshot, blob) = (repo.join("snapshots/abc"), repo.join("blobs/0123"));
    std::fs::create_dir_all(&snapshot)?;
    std::fs::create_dir_all(repo.join("blobs"))?;
    std::fs::write(&blob, b"GGUF")?;
    std::os::unix::fs::symlink("../../blobs/0123", snapshot.join("m-Q4_0.gguf"))?;
    let file = snapshot.join("m-Q8_0.gguf");
    std::fs::write(&file, b"GGUF")?;
    let store = Store::new(paths);
    let (snapshot, blob, file) =
        (snapshot.canonicalize()?, blob.canonicalize()?, file.canonicalize()?);
    let imports = [
        store.gguf_import_dir(&snapshot),
        store.gguf_import_dir(&blob),
        store.gguf_import_dir(&file),
        imports_dir.join(crate::config::folder_key(&file.with_extension(""))),
    ];
    for import in &imports {
        std::fs::create_dir_all(import)?;
        std::fs::write(import.join("config.json"), b"{}")?;
    }
    let removal = removing(&store, "owner/gguf").await?;
    assert!(removal.removed);
    assert_eq!(removal.freed_bytes, 16);
    assert!(!repo.exists() && imports.iter().all(|import| !import.exists()));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// A link in a cache resolving out of it names a file that stays: its
/// import stays too, as a model configured at that file loads from it.
#[cfg(unix)]
#[tokio::test]
async fn removing_a_cache_keeps_imports_of_files_outside_it() -> Result<()> {
    let root = root("remove-outside");
    let paths = crate::config::Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    );
    let outside = root.join("models/m-Q4_0.gguf");
    std::fs::create_dir_all(root.join("models"))?;
    std::fs::write(&outside, b"GGUF")?;
    let snapshot = paths.hub_cache_dir.join("models--Owner--Linked/snapshots/abc");
    std::fs::create_dir_all(&snapshot)?;
    std::os::unix::fs::symlink(&outside, snapshot.join("m-Q4_0.gguf"))?;
    let store = Store::new(paths);
    let import = store.gguf_import_dir(&outside.canonicalize()?);
    std::fs::create_dir_all(&import)?;
    assert!(removing(&store, "owner/linked").await?.removed);
    assert!(import.exists() && outside.exists() && !snapshot.exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

mod scope;
