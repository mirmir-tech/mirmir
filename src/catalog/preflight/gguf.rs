//! Admission of a Hub GGUF variant from its headers alone: each shard's
//! prefix is fetched by range requests, growing until its header parses.

use std::path::PathBuf;

use libmir::{
    RemoteModelContract,
    gguf::{GgufHeader, GgufModel, ParseError},
};

use super::{HUB_BASE, file_url, range};
use crate::{
    catalog::hub::GgufVariant,
    error::{Error, Result},
};

/// The first request: most headers, vocabulary included, fit in 8 MiB.
const FIRST_BYTES: u64 = 8 * 1024 * 1024;
/// Headers larger than this are refused rather than fetched.
const MAX_HEADER_BYTES: u64 = 128 * 1024 * 1024;

/// The variant's contract, or libmir's refusal of its headers, and the
/// header bytes fetched for it; an error when the headers could not be read.
pub(in crate::catalog) async fn inspect(
    client: &reqwest::Client,
    token: Option<&str>,
    repo_id: &str,
    revision: &str,
    variant: &GgufVariant,
) -> Result<(libmir::Result<RemoteModelContract>, u64)> {
    let base = match reqwest::Url::parse(HUB_BASE) {
        Ok(base) => base,
        Err(error) => return Err(Error::Config(format!("invalid Hub base URL: {error}"))),
    };
    let mut shards = Vec::with_capacity(variant.shards.len());
    let mut fetched = 0;
    for shard in &variant.shards {
        let url = file_url(&base, repo_id, revision, &shard.path)?;
        let size = shard
            .size
            .ok_or_else(|| Error::Config(format!("{} was listed without a size", shard.path)))?;
        let mut bytes = Vec::new();
        let mut wanted = FIRST_BYTES.min(size);
        let header = loop {
            let start = bytes.len();
            let end = usize::try_from(wanted)?.saturating_sub(1);
            bytes.extend(range::fetch(client, token, url.clone(), start, end).await?);
            match GgufHeader::parse(&bytes, Some(size)) {
                Ok(header) => break header,
                Err(ParseError::Truncated { needed })
                    if needed <= MAX_HEADER_BYTES && wanted < size =>
                {
                    // Fetch what the parser asks for, at least doubling.
                    wanted = needed.max(wanted.saturating_mul(2)).min(size);
                },
                Err(ParseError::Truncated { needed }) => {
                    return Err(Error::Config(format!(
                        "{} needs {needed} header bytes, beyond the file or the preflight limit",
                        shard.path
                    )));
                },
                Err(ParseError::Invalid(error)) => return Err(libmir::Error::from(error).into()),
            }
        };
        fetched += u64::try_from(bytes.len())?;
        shards.push((PathBuf::from(&shard.path), header));
    }
    let contract = GgufModel::from_headers(shards)
        .map_err(libmir::Error::from)
        .and_then(|model| RemoteModelContract::inspect_gguf(&model));
    Ok((contract, fetched))
}
