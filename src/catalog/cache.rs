use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedModel {
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub repo_path: PathBuf,
    pub snapshot: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialDownload {
    pub repo_id: String,
    pub downloaded_bytes: u64,
}

pub fn discover_cached_models() -> Vec<CachedModel> {
    discover_in(&hf_hub::resolve_cache_dir())
}

pub fn discover_cached_models_in(cache: &Path) -> Vec<CachedModel> {
    discover_in(cache)
}

pub fn discover_partial_downloads_in(cache: &Path) -> Vec<PartialDownload> {
    let Ok(entries) = fs::read_dir(cache) else {
        return Vec::new();
    };
    let mut downloads = entries
        .flatten()
        .filter_map(|entry| partial_download(&entry.path()))
        .collect::<Vec<_>>();
    downloads.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
    downloads
}

fn discover_in(cache: &Path) -> Vec<CachedModel> {
    let Ok(entries) = fs::read_dir(cache) else {
        return Vec::new();
    };
    let mut models = entries
        .flatten()
        .filter_map(|entry| cached_model(&entry.path()))
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
    models
}

fn cached_model(repo_path: &Path) -> Option<CachedModel> {
    if has_incomplete(repo_path) {
        return None;
    }
    let repo_id = repo_id(repo_path)?;
    let snapshots = repo_path.join("snapshots");
    let (revision, commit) = main_revision(repo_path)
        .filter(|(_, commit)| snapshots.join(commit).is_dir())
        .or_else(|| newest_snapshot(&snapshots).map(|commit| (commit.clone(), commit)))?;
    Some(CachedModel {
        repo_id,
        revision,
        repo_path: repo_path.to_owned(),
        snapshot: snapshots.join(&commit),
        commit,
    })
}

fn has_incomplete(repo_path: &Path) -> bool {
    fs::read_dir(repo_path.join("blobs")).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.path().extension().and_then(|value| value.to_str()) == Some("incomplete")
        })
    })
}

fn partial_download(repo_path: &Path) -> Option<PartialDownload> {
    let repo_id = repo_id(repo_path)?;
    cached_model(repo_path).is_none().then(|| PartialDownload {
        repo_id,
        downloaded_bytes: directory_size(&repo_path.join("blobs")),
    })
}

fn repo_id(repo_path: &Path) -> Option<String> {
    let encoded = repo_path.file_name()?.to_str()?.strip_prefix("models--")?;
    Some(
        encoded
            .split_once("--")
            .map_or_else(|| encoded.to_owned(), |(owner, name)| format!("{owner}/{name}")),
    )
}

fn directory_size(path: &Path) -> u64 {
    fs::read_dir(path).map_or(0, |entries| {
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

fn main_revision(repo_path: &Path) -> Option<(String, String)> {
    let commit = fs::read_to_string(repo_path.join("refs/main")).ok()?;
    let commit = commit.trim();
    (!commit.is_empty()).then(|| ("main".to_owned(), commit.to_owned()))
}

fn newest_snapshot(snapshots: &Path) -> Option<String> {
    fs::read_dir(snapshots)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .max_by_key(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })?
        .file_name()
        .into_string()
        .ok()
}

#[cfg(test)]
mod tests {
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

        let models = discover_in(&root);
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
        fs::create_dir_all(root.join("models--Org--Unsupported/snapshots/deadbeef"))?;

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
}
