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

/// Whose cache a directory is: mirmir writes a revision reference only
/// once every file of a download is in place; another tool's cache follows
/// its own conventions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cache {
    Managed,
    External,
}

/// Models in the user's own Hugging Face cache, when the paths name one.
pub fn discover_external_models(paths: &crate::config::Paths) -> Vec<CachedModel> {
    paths
        .external_hub_cache
        .as_deref()
        .map(|cache| discover_in(cache, Cache::External))
        .unwrap_or_default()
}

pub fn discover_cached_models_in(cache: &Path) -> Vec<CachedModel> {
    discover_in(cache, Cache::Managed)
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

fn discover_in(cache: &Path, kind: Cache) -> Vec<CachedModel> {
    let Ok(entries) = fs::read_dir(cache) else {
        return Vec::new();
    };
    let mut models = entries
        .flatten()
        .filter_map(|entry| cached_model(&entry.path(), kind))
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.repo_id.cmp(&right.repo_id));
    models
}

fn cached_model(repo_path: &Path, kind: Cache) -> Option<CachedModel> {
    if has_incomplete(repo_path) || (kind == Cache::Managed && !has_reference(repo_path)) {
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

/// A revision reference naming one of the repository's snapshots.
fn has_reference(repo_path: &Path) -> bool {
    fs::read_dir(repo_path.join("refs")).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            fs::read_to_string(entry.path()).is_ok_and(|commit| {
                let commit = commit.trim();
                !commit.is_empty() && repo_path.join("snapshots").join(commit).is_dir()
            })
        })
    })
}

fn has_incomplete(repo_path: &Path) -> bool {
    fs::read_dir(repo_path.join("blobs")).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.path().extension().and_then(|value| value.to_str()) == Some("incomplete")
        })
    })
}

/// The bytes `repo_id`'s managed cache holds; `None` without one.
#[must_use]
pub fn cached_repository_bytes(cache: &Path, repo_id: &str) -> Option<u64> {
    let repo_path = cache.join(format!("models--{}", repo_id.replace('/', "--")));
    repo_path.is_dir().then(|| directory_size(&repo_path.join("blobs")))
}

fn partial_download(repo_path: &Path) -> Option<PartialDownload> {
    let repo_id = repo_id(repo_path)?;
    cached_model(repo_path, Cache::Managed).is_none().then(|| PartialDownload {
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
mod tests;
