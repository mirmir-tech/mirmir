mod cache;
mod index;
mod metadata;
mod presentation;
mod range;

use std::{collections::BTreeSet, path::PathBuf};

pub(super) use cache::Cache;
use futures_util::TryStreamExt;
use hf_hub::{repository::RepoTreeEntry, split_id};
use libmir::{RemoteModelContract, TensorCatalog, safetensors_header_len};
pub use metadata::RemoteModelMetadata;
pub(super) use presentation::{apply as apply_to_model, failure as apply_failure};

use super::download;
use crate::{
    config::Store,
    error::{Error, Result},
};

const HUB_BASE: &str = "https://huggingface.co";
const MAX_TOTAL_PREFLIGHT_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteHeaderPreflight {
    pub catalog: TensorCatalog,
    pub metadata: RemoteModelMetadata,
    pub contract: Option<RemoteModelContract>,
    pub contract_error: Option<String>,
    pub fetched_bytes: u64,
    pub files: Vec<String>,
    pub revision: String,
}

#[derive(Debug)]
struct RepositoryFiles {
    weights: Vec<String>,
    index: Option<(String, usize)>,
    metadata: metadata::Files,
}

pub(super) async fn inspect_repository(
    client: &reqwest::Client,
    store: &Store,
    token: Option<&str>,
    repo_id: &str,
    revision: &str,
) -> Result<RemoteHeaderPreflight> {
    let listed = repository_files(store, repo_id, revision).await?;
    let base = match reqwest::Url::parse(HUB_BASE) {
        Ok(base) => base,
        Err(error) => return Err(Error::Config(format!("invalid Hub base URL: {error}"))),
    };
    let (files, index_bytes) =
        resolve_weight_files(client, token, &base, repo_id, revision, &listed).await?;
    let (metadata, metadata_bytes) =
        metadata::fetch(client, token, &base, repo_id, revision, &listed.metadata).await?;
    let mut result =
        inspect_files_at(client, token, &base, repo_id, revision, &files, metadata).await?;
    match RemoteModelContract::inspect(
        &result.metadata.config,
        &result.catalog,
        result.metadata.task_metadata(),
    ) {
        Ok(Some(contract)) => result.contract = Some(contract),
        Ok(None) => {},
        Err(error) => result.contract_error = Some(error.to_string()),
    }
    let extra = index_bytes
        .checked_add(metadata_bytes)
        .ok_or_else(|| Error::Config("preflight byte count overflow".into()))?;
    result.fetched_bytes =
        u64::try_from(checked_total(usize::try_from(result.fetched_bytes)?, extra)?)?;
    Ok(result)
}

pub(super) async fn resolve_revision(
    store: &Store,
    repo_id: &str,
    revision: &str,
) -> Result<String> {
    let hf = download::client(store)?;
    let (owner, name) = split_id(repo_id);
    let model = hf.model(owner, name);
    let metadata = model
        .get_file_metadata()
        .filepath("config.json")
        .revision(revision)
        .send()
        .await?;
    Ok(metadata.commit_hash)
}

async fn repository_files(store: &Store, repo_id: &str, revision: &str) -> Result<RepositoryFiles> {
    let hf = download::client(store)?;
    let (owner, name) = split_id(repo_id);
    let model = hf.model(owner, name);
    let tree = model.list_tree().revision(revision).recursive(true).send()?;
    futures_util::pin_mut!(tree);
    let mut weights = Vec::new();
    let mut index = None;
    let mut metadata = metadata::Files::default();
    while let Some(entry) = tree.try_next().await? {
        if let RepoTreeEntry::File { path, size, .. } = entry {
            metadata.observe(&path, size)?;
            if path.ends_with(".safetensors") {
                weights.push(path);
            } else if path == index::FILENAME {
                index = Some((path, usize::try_from(size)?));
            }
        }
    }
    weights.sort();
    weights.dedup();
    if weights.is_empty() {
        return Err(Error::Config(format!("model `{repo_id}` contains no SafeTensors files")));
    }
    metadata.validate(repo_id)?;
    Ok(RepositoryFiles { weights, index, metadata })
}

async fn resolve_weight_files(
    client: &reqwest::Client,
    token: Option<&str>,
    base: &reqwest::Url,
    repo_id: &str,
    revision: &str,
    listed: &RepositoryFiles,
) -> Result<(Vec<String>, usize)> {
    let Some((filename, size)) = &listed.index else {
        return Ok((listed.weights.clone(), 0));
    };
    index::validate_size(*size)?;
    let url = file_url(base, repo_id, revision, filename)?;
    let bytes = range::fetch(client, token, url, 0, size.saturating_sub(1)).await?;
    Ok((index::shard_files(&bytes, &listed.weights)?, bytes.len()))
}

async fn inspect_files_at(
    client: &reqwest::Client,
    token: Option<&str>,
    base: &reqwest::Url,
    repo_id: &str,
    revision: &str,
    files: &[String],
    metadata: RemoteModelMetadata,
) -> Result<RemoteHeaderPreflight> {
    let mut tensors = Vec::new();
    let mut names = BTreeSet::new();
    let mut fetched = 0_usize;
    for file in files {
        let url = file_url(base, repo_id, revision, file)?;
        let prefix = range::fetch(client, token, url.clone(), 0, 7).await?;
        let header_len = inference(safetensors_header_len(&prefix))?;
        fetched = checked_total(fetched, 8_usize.saturating_add(header_len))?;
        let end = 8_usize
            .checked_add(header_len)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| Error::Config("SafeTensors header range overflow".into()))?;
        let header = range::fetch(client, token, url, 8, end).await?;
        let mut bytes = prefix;
        bytes.extend_from_slice(&header);
        let catalog =
            inference(TensorCatalog::from_safetensors_header(PathBuf::from(file), &bytes))?;
        for tensor in catalog.tensors {
            if !names.insert(tensor.name.clone()) {
                return Err(Error::Config(format!(
                    "tensor `{}` is declared by multiple SafeTensors shards",
                    tensor.name
                )));
            }
            tensors.push(tensor);
        }
    }
    tensors.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(RemoteHeaderPreflight {
        catalog: TensorCatalog { tensors },
        metadata,
        contract: None,
        contract_error: None,
        fetched_bytes: u64::try_from(fetched)?,
        files: files.to_vec(),
        revision: revision.to_owned(),
    })
}

fn checked_total(current: usize, additional: usize) -> Result<usize> {
    let total = current
        .checked_add(additional)
        .ok_or_else(|| Error::Config("preflight byte count overflow".into()))?;
    if total > MAX_TOTAL_PREFLIGHT_BYTES {
        return Err(Error::Config(format!(
            "remote inspection exceeds the {MAX_TOTAL_PREFLIGHT_BYTES} byte preflight limit"
        )));
    }
    Ok(total)
}

pub(super) fn file_url(
    base: &reqwest::Url,
    repo_id: &str,
    revision: &str,
    filename: &str,
) -> Result<reqwest::Url> {
    let mut url = base.clone();
    let Ok(mut segments) = url.path_segments_mut() else {
        return Err(Error::Config("Hub base URL cannot hold path segments".into()));
    };
    segments.pop_if_empty();
    segments.extend(repo_id.split('/'));
    segments.push("resolve").push(revision);
    segments.extend(filename.split('/'));
    drop(segments);
    Ok(url)
}

fn inference<T, E>(result: std::result::Result<T, E>) -> Result<T>
where
    libmir::Error: From<E>,
{
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(libmir::Error::from(error).into()),
    }
}

#[cfg(test)]
mod tests;
