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

/// The commit `revision` of `repo_id` names, read from the metadata of the
/// first of `files` that reports it.
pub async fn commit(
    store: &Store,
    repo_id: &str,
    revision: &str,
    files: &[ListedFile],
) -> Result<String> {
    let hf = client(store)?;
    let (owner, name) = split_id(repo_id);
    resolve_commit(&hf.model(owner, name), revision, files).await
}

/// Downloads `files` of `repo_id` at `commit` of `revision` into its
/// snapshot, which it returns.
pub async fn snapshot(
    store: &Store,
    (repo_id, revision, commit): (&str, &str, String),
    files: Vec<ListedFile>,
    updates: &mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<PathBuf> {
    let metadata = files
        .into_iter()
        .map(|file| hf_hub::repository::FileMetadataInfo {
            location: Some(download_url(repo_id, revision, &file.path)),
            filename: file.path,
            etag: file.etag,
            commit_hash: commit.clone(),
            xet_hash: None,
            file_size: file.size,
        })
        .collect::<Vec<_>>();
    let root = store.paths().hub_cache_dir.join(format!("models--{}", model_key(repo_id)?));
    let snapshot = root.join("snapshots").join(&commit);
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
    let reference = root.join("refs").join(revision);
    if let Some(parent) = reference.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(reference, format!("{commit}\n")).await?;
    Ok(snapshot)
}

async fn resolve_commit(
    repository: &hf_hub::HFRepository<hf_hub::RepoTypeModel>,
    revision: &str,
    files: &[ListedFile],
) -> Result<String> {
    for file in files {
        if let Ok(value) = repository
            .get_file_metadata()
            .filepath(&file.path)
            .revision(revision)
            .send()
            .await
        {
            return Ok(value.commit_hash);
        }
    }
    Err(Error::Config("could not resolve repository commit".to_owned()))
}

fn download_url(repo_id: &str, revision: &str, filename: &str) -> String {
    let mut url = reqwest::Url::parse("https://huggingface.co").expect("static Hub URL is valid");
    {
        let mut segments = url.path_segments_mut().expect("Hub URL can hold path segments");
        repo_id.split('/').for_each(|segment| {
            segments.push(segment);
        });
        segments.push("resolve").push(revision);
        filename.split('/').for_each(|segment| {
            segments.push(segment);
        });
    }
    url.to_string()
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
