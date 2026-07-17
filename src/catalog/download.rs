use hf_hub::{HFClient, split_id};
use tokio::sync::mpsc;

use super::{cache::discover_cached_models, progress::Reporter};
use crate::{
    config::{ModelConfig, Store, model_key},
    error::{Error, Result},
};

#[derive(Debug, Clone)]
pub struct TransferUpdate {
    pub phase: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Removal {
    pub removed: bool,
    pub freed_bytes: u64,
}

pub async fn pull(
    store: &Store,
    repo_id: &str,
    revision: Option<&str>,
    updates: mpsc::Sender<TransferUpdate>,
) -> Result<ModelConfig> {
    drop(model_key(repo_id)?);
    let revision = revision.unwrap_or("main");
    let client = client(store)?;
    let (owner, name) = split_id(repo_id);
    send(&updates, "resolving", 0, None, format!("resolving {repo_id}@{revision}")).await;
    let snapshot = client
        .model(owner, name)
        .snapshot_download()
        .revision(revision)
        .allow_patterns(patterns())
        .max_workers(8)
        .progress(Reporter::new(updates.clone()))
        .send()
        .await?;
    send(
        &updates,
        "validating",
        directory_size(&snapshot),
        None,
        format!("validating snapshot for {repo_id}"),
    )
    .await;
    drop(libmir::ModelDescriptor::inspect(
        &snapshot,
        libmir::GenerationOverrides::default(),
    )?);
    let model = store.save_hub_model(repo_id, revision, snapshot.clone())?;
    send(
        &updates,
        "available",
        directory_size(&snapshot),
        None,
        format!("saved model configuration for {repo_id}"),
    )
    .await;
    Ok(model)
}

pub async fn remove(store: &Store, repo_id: &str) -> Result<Removal> {
    drop(model_key(repo_id)?);
    let cache = client(store)?.scan_cache().send().await?;
    let managed = cache
        .repos
        .into_iter()
        .find(|repo| repo.repo_type == "model" && repo.repo_id == repo_id);
    let external = || {
        discover_cached_models()
            .into_iter()
            .find(|model| model.repo_id == repo_id)
            .map(|model| (model.repo_path, hf_hub::resolve_cache_dir(), None))
    };
    let target = managed.map_or_else(external, |repo| {
        Some((repo.repo_path, store.paths().hub_cache_dir.clone(), Some(repo.size_on_disk)))
    });
    let removed_cache = target.is_some();
    let freed_bytes = if let Some((path, root, known_size)) = target {
        remove_cache_repo(path, root, known_size).await?
    } else {
        0
    };
    let removed_config = store.remove_hub_model_config(repo_id)?;
    Ok(Removal {
        removed: removed_cache || removed_config,
        freed_bytes,
    })
}

async fn remove_cache_repo(
    path: std::path::PathBuf,
    cache: std::path::PathBuf,
    known_size: Option<u64>,
) -> Result<u64> {
    tokio::task::spawn_blocking(move || {
        let path = validated_cache_repo(&path, &cache)?;
        let size = known_size.unwrap_or_else(|| directory_size(&path));
        std::fs::remove_dir_all(path)?;
        Ok(size)
    })
    .await?
}

fn client(store: &Store) -> Result<HFClient> {
    let mut builder = HFClient::builder()
        .cache_dir(&store.paths().hub_cache_dir)
        .user_agent(concat!("mirmir/", env!("CARGO_PKG_VERSION")));
    if let Some(token) = store.hf_token()? {
        builder = builder.token(token);
    }
    Ok(builder.build()?)
}

fn patterns() -> Vec<String> {
    [
        "*.json",
        "**/*.json",
        "*.safetensors",
        "**/*.safetensors",
        "*.model",
        "**/*.model",
        "merges.txt",
        "vocab.*",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn validated_cache_repo(
    path: &std::path::Path,
    cache: &std::path::Path,
) -> Result<std::path::PathBuf> {
    let cache = cache.canonicalize()?;
    let path = path.canonicalize()?;
    let is_model_repo = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("models--"));
    if path.parent() == Some(cache.as_path()) && is_model_repo {
        return Ok(path);
    }
    Err(Error::Config(format!(
        "refusing to remove model cache outside {}",
        cache.display()
    )))
}

async fn send(
    sender: &mpsc::Sender<TransferUpdate>,
    phase: &'static str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    message: String,
) {
    drop(
        sender
            .send(TransferUpdate {
                phase,
                downloaded_bytes,
                total_bytes,
                message,
            })
            .await,
    );
}

fn directory_size(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path).map_or(0, |entries| {
        entries.flatten().fold(0, |total, entry| {
            let path = entry.path();
            total.saturating_add(if path.is_dir() {
                directory_size(&path)
            } else {
                entry.metadata().map_or(0, |metadata| metadata.len())
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_direct_model_repositories_in_cache() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "mirmir-remove-cache-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
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
        let root = std::env::temp_dir().join(format!(
            "mirmir-remove-external-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let cache = root.join("hub");
        let model = cache.join("models--Qwen--External");
        let weights = model.join("snapshots/abc/model.safetensors");
        std::fs::create_dir_all(weights.parent().unwrap_or(&model))?;
        std::fs::write(&weights, b"weights")?;

        let freed = remove_cache_repo(model.clone(), cache, None).await?;
        assert_eq!(freed, 7);
        assert!(!model.exists());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
