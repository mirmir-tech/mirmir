use std::collections::{BTreeMap, HashMap};

use libmir::{
    RemoteTaskMetadata,
    models::{execution::EmbeddingTask, tokenizer::TokenizerAssets},
};
use serde_json::Value;

use crate::error::{Error, Result};

const CONFIG: &str = "config.json";
const TOKENIZER: &str = "tokenizer_config.json";
const MODULES: &str = "modules.json";
const SENTENCE: &str = "config_sentence_transformers.json";
const PROCESSOR: &str = "processor_config.json";
const PREPROCESSOR: &str = "preprocessor_config.json";
const TOKENIZER_ASSETS: [&str; 7] = [
    "tokenizer.json",
    "tokenizer.model",
    "vocab.json",
    "merges.txt",
    TOKENIZER,
    "added_tokens.json",
    "special_tokens_map.json",
];
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct RemoteModelMetadata {
    pub config: Value,
    pub tokenizer_config: Option<Value>,
    pub modules: Option<Value>,
    pub pooling: Option<Value>,
    pub sentence_transformers: Option<Value>,
    pub processor_config: Option<Value>,
    pub tokenizer_assets: TokenizerAssets,
}

#[derive(Debug, Default)]
pub(super) struct Files {
    pub config: Option<File>,
    pub tokenizer: Option<File>,
    pub modules: Option<File>,
    pub sentence_transformers: Option<File>,
    pub processor: Option<File>,
    pub preprocessor: Option<File>,
    tokenizer_assets: BTreeMap<String, u64>,
    nested_configs: HashMap<String, File>,
}

#[derive(Debug)]
pub(super) struct File {
    pub path: String,
    pub size: usize,
}

impl Files {
    pub fn observe(&mut self, path: &str, size: u64) -> Result<()> {
        if TOKENIZER_ASSETS.contains(&path) {
            let _previous = self.tokenizer_assets.insert(path.to_owned(), size);
        }
        let file = || -> Result<File> {
            Ok(File {
                path: path.to_owned(),
                size: usize::try_from(size)?,
            })
        };
        match path {
            CONFIG => self.config = Some(file()?),
            TOKENIZER => self.tokenizer = Some(file()?),
            MODULES => self.modules = Some(file()?),
            SENTENCE => self.sentence_transformers = Some(file()?),
            PROCESSOR => self.processor = Some(file()?),
            PREPROCESSOR => self.preprocessor = Some(file()?),
            path if path.ends_with("/config.json") => {
                let _previous = self.nested_configs.insert(path.to_owned(), file()?);
            },
            _ => {},
        }
        Ok(())
    }

    pub fn validate(&self, repo_id: &str) -> Result<()> {
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| Error::Config(format!("model `{repo_id}` has no `{CONFIG}`")))?;
        config.validate_size()?;
        self.tokenizer.as_ref().map(File::validate_size).transpose()?;
        self.modules.as_ref().map(File::validate_size).transpose()?;
        self.sentence_transformers.as_ref().map(File::validate_size).transpose()?;
        self.processor.as_ref().map(File::validate_size).transpose()?;
        self.preprocessor.as_ref().map(File::validate_size).transpose()?;
        Ok(())
    }
}

impl File {
    pub fn validate_size(&self) -> Result<()> {
        if self.size == 0 {
            return Err(Error::Config(format!("`{}` is empty", self.path)));
        }
        if self.size > MAX_BYTES {
            return Err(Error::Config(format!(
                "`{}` exceeds the {MAX_BYTES} byte metadata limit",
                self.path
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn parse(config: &[u8], tokenizer: Option<&[u8]>) -> Result<RemoteModelMetadata> {
    Ok(RemoteModelMetadata {
        config: serde_json::from_slice(config)?,
        tokenizer_config: tokenizer.map(serde_json::from_slice).transpose()?,
        modules: None,
        pooling: None,
        sentence_transformers: None,
        processor_config: None,
        tokenizer_assets: inference(TokenizerAssets::discover(&BTreeMap::from([(
            "tokenizer.json".into(),
            0,
        )])))?,
    })
}

impl RemoteModelMetadata {
    pub const fn task_metadata(&self) -> RemoteTaskMetadata<'_> {
        RemoteTaskMetadata {
            modules: self.modules.as_ref(),
            pooling: self.pooling.as_ref(),
            sentence_transformers: self.sentence_transformers.as_ref(),
            processor: self.processor_config.as_ref(),
        }
    }
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    token: Option<&str>,
    base: &reqwest::Url,
    repo_id: &str,
    revision: &str,
    files: &Files,
) -> Result<(RemoteModelMetadata, usize)> {
    let tokenizer_assets = inference(TokenizerAssets::discover(&files.tokenizer_assets))?;
    let config_file = files
        .config
        .as_ref()
        .ok_or_else(|| Error::Config("remote config metadata is missing".into()))?;
    let config =
        super::range::fetch_file(client, token, base, repo_id, revision, config_file).await?;
    let tokenizer =
        fetch_optional(client, token, base, repo_id, revision, files.tokenizer.as_ref()).await?;
    let modules =
        fetch_optional(client, token, base, repo_id, revision, files.modules.as_ref()).await?;
    let sentence = fetch_optional(
        client,
        token,
        base,
        repo_id,
        revision,
        files.sentence_transformers.as_ref(),
    )
    .await?;
    let processor = fetch_optional(
        client,
        token,
        base,
        repo_id,
        revision,
        files.processor.as_ref().or(files.preprocessor.as_ref()),
    )
    .await?;
    let modules_value = modules.as_deref().map(serde_json::from_slice).transpose()?;
    let pooling = match inference(
        modules_value.as_ref().map(EmbeddingTask::pooling_config_path).transpose(),
    )? {
        Some(Some(path)) => {
            let file = files.nested_configs.get(&path).ok_or_else(|| {
                Error::Config(format!("Sentence Transformers pooling config `{path}` is missing"))
            })?;
            Some(super::range::fetch_file(client, token, base, repo_id, revision, file).await?)
        },
        Some(None) | None => None,
    };
    let fetched = std::iter::once(&config)
        .chain(tokenizer.iter())
        .chain(modules.iter())
        .chain(sentence.iter())
        .chain(processor.iter())
        .chain(pooling.iter())
        .try_fold(0_usize, |total, bytes| total.checked_add(bytes.len()))
        .ok_or_else(|| Error::Config("metadata byte count overflow".into()))?;
    Ok((
        RemoteModelMetadata {
            config: serde_json::from_slice(&config)?,
            tokenizer_config: tokenizer.as_deref().map(serde_json::from_slice).transpose()?,
            modules: modules_value,
            pooling: pooling.as_deref().map(serde_json::from_slice).transpose()?,
            sentence_transformers: sentence.as_deref().map(serde_json::from_slice).transpose()?,
            processor_config: processor.as_deref().map(serde_json::from_slice).transpose()?,
            tokenizer_assets,
        },
        fetched,
    ))
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

async fn fetch_optional(
    client: &reqwest::Client,
    token: Option<&str>,
    base: &reqwest::Url,
    repo_id: &str,
    revision: &str,
    file: Option<&File>,
) -> Result<Option<Vec<u8>>> {
    match file {
        Some(file) => Ok(Some(
            super::range::fetch_file(client, token, base, repo_id, revision, file).await?,
        )),
        None => Ok(None),
    }
}
