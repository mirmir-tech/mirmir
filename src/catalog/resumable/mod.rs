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

pub async fn snapshot(
    store: &Store,
    repo_id: &str,
    revision: &str,
    updates: &mpsc::Sender<TransferUpdate>,
    cancellation: &CancellationToken,
) -> Result<PathBuf> {
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
                    Some((path, etag, size))
                },
                RepoTreeEntry::Directory { .. } => None,
            })
        })
        .try_collect::<Vec<_>>()
        .await?;
    if files.is_empty() {
        return Err(Error::Config(format!("model `{repo_id}` contains no files")));
    }
    let commit = resolve_commit(&repository, revision, &files).await?;
    let metadata = files
        .into_iter()
        .map(|(filename, etag, file_size)| hf_hub::repository::FileMetadataInfo {
            location: Some(download_url(repo_id, revision, &filename)),
            filename,
            etag,
            commit_hash: commit.clone(),
            xet_hash: None,
            file_size,
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
    files: &[(String, String, u64)],
) -> Result<String> {
    for (filename, _, _) in files {
        if let Ok(value) = repository
            .get_file_metadata()
            .filepath(filename)
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
