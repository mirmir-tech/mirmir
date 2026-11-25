mod file;

use std::path::{Path, PathBuf};

use futures_util::TryStreamExt;
use hf_hub::{repository::RepoTreeEntry, split_id};
use libmir::CancellationToken;
use tokio::{fs, sync::mpsc};

use self::file::Download;
use super::{
    TransferUpdate,
    download::{client, send},
};
use crate::{
    config::{Store, model_key},
    error::{Error, Result},
};

/// One file of a repository revision, as the download needs it.
#[derive(Debug, Clone)]
pub struct ListedFile {
    pub path: String,
    pub etag: String,
    pub size: u64,
}

/// Every file of `repo_id` at `revision`.
pub async fn listing(store: &Store, repo_id: &str, revision: &str) -> Result<Vec<ListedFile>> {
    let hf = client(store)?;
    let (owner, name) = split_id(repo_id);
    let repository = hf.model(owner, name);
    let tree = repository.list_tree().revision(revision).recursive(true).send()?;
    let files = tree
        .try_filter_map(|entry| async move {
            Ok(match entry {
                RepoTreeEntry::File { oid, size, path, lfs, xet_hash, .. } => {
                    let etag =
                        xet_hash.or_else(|| lfs.and_then(|value| value.sha256)).unwrap_or(oid);
                    Some(ListedFile { path, etag, size })
                },
                RepoTreeEntry::Directory { .. } => None,
            })
        })
        .try_collect::<Vec<_>>()
        .await?;
    if files.is_empty() {
        return Err(Error::Config(format!("model `{repo_id}` contains no files")));
    }
    Ok(files)
}

/// A revision pinned to the commit it named when the pull began. Listing,
/// header checks and file requests all use the commit, so a branch that
/// moves during a transfer or its resume cannot mix two revisions' bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    /// The revision as asked for, such as `main`, which the cache's
    /// `refs` and the saved configuration name.
    pub requested: String,
    /// The immutable commit it resolved to.
    pub commit: String,
}

/// Resolves `revision` of `repo_id` to its commit once; a full commit hash
/// is its own.
pub async fn pin(store: &Store, repo_id: &str, revision: &str) -> Result<Pinned> {
    let commit = if is_commit(revision) {
        revision.to_owned()
    } else {
        let hf = client(store)?;
        let (owner, name) = split_id(repo_id);
        hf.model(owner, name)
            .info()
            .revision(revision)
            .send()
            .await?
            .sha
            .ok_or_else(|| {
                Error::Config(format!("the Hub names no commit for {repo_id}@{revision}"))
            })?
    };
    Ok(Pinned { requested: revision.to_owned(), commit })
}

fn is_commit(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Downloads `files` of `repo_id` at `pinned`'s commit into its snapshot,
/// which it returns.
pub async fn snapshot(
    store: &Store,
    (repo_id, pinned): (&str, &Pinned),
    files: Vec<ListedFile>,
    updates: &mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<PathBuf> {
    let endpoint = client(store)?.endpoint().to_owned();
    transfer(store, &endpoint, (repo_id, pinned), files, updates, cancellation).await
}

/// [`snapshot`] from the Hub at `endpoint`.
async fn transfer(
    store: &Store,
    endpoint: &str,
    (repo_id, pinned): (&str, &Pinned),
    files: Vec<ListedFile>,
    updates: &mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<PathBuf> {
    let metadata = files
        .into_iter()
        .map(|file| {
            Ok(hf_hub::repository::FileMetadataInfo {
                location: Some(download_url(endpoint, repo_id, &pinned.commit, &file.path)?),
                filename: file.path,
                etag: file.etag,
                commit_hash: pinned.commit.clone(),
                xet_hash: None,
                file_size: file.size,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let root = store.paths().hub_cache_dir.join(format!("models--{}", model_key(repo_id)?));
    let snapshot = root.join("snapshots").join(&pinned.commit);
    let total = metadata.iter().map(|file| file.file_size).sum();
    let mut completed = completed_bytes(&root, &metadata);
    send(
        updates,
        super::TransferPhase::Downloading,
        completed,
        Some(total),
        format!("{} files", metadata.len()),
    )
    .await;
    let transfer = Download::new(store, &root, &snapshot, total, updates, cancellation);
    for file in metadata {
        completed = transfer.file(file, completed).await?;
    }
    let reference = root.join("refs").join(&pinned.requested);
    if let Some(parent) = reference.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(reference, format!("{}\n", pinned.commit)).await?;
    Ok(snapshot)
}

/// `<endpoint>/<repo_id>/resolve/<commit>/<filename>`.
fn download_url(endpoint: &str, repo_id: &str, commit: &str, filename: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(endpoint)
        .map_err(|error| Error::Config(format!("invalid Hub endpoint {endpoint}: {error}")))?;
    url.path_segments_mut()
        .map_err(|()| Error::Config(format!("Hub endpoint {endpoint} cannot hold a path")))?
        .pop_if_empty()
        .extend(repo_id.split('/'))
        .push("resolve")
        .push(commit)
        .extend(filename.split('/'));
    Ok(url.to_string())
}

fn completed_bytes(root: &Path, files: &[hf_hub::repository::FileMetadataInfo]) -> u64 {
    files
        .iter()
        .map(|file| {
            let final_blob = root.join("blobs").join(&file.etag);
            let partial = root.join("blobs").join(format!("{}.incomplete", file.etag));
            std::fs::metadata(final_blob)
                .or_else(|_| std::fs::metadata(partial))
                .map_or(0, |metadata| metadata.len().min(file.file_size))
        })
        .sum()
}

#[cfg(test)]
mod tests;
