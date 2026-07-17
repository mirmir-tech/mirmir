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

pub fn discover_cached_models() -> Vec<CachedModel> {
    discover_in(&hf_hub::resolve_cache_dir())
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
    let encoded = repo_path.file_name()?.to_str()?.strip_prefix("models--")?;
    let repo_id = encoded
        .split_once("--")
        .map_or_else(|| encoded.to_owned(), |(owner, name)| format!("{owner}/{name}"));
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
}
