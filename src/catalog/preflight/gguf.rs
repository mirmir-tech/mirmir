//! Admission of a Hub GGUF variant from its headers alone: each shard's
//! prefix is fetched by range requests, growing until its header parses,
//! with the few small vectors its configuration derives from (`rope_freqs`).

use std::path::PathBuf;

use libmir::{
    RemoteModelContract,
    gguf::{ConfigurationVector, GgufHeader, GgufModel, ParseError},
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
/// Configuration vectors larger than this are refused rather than fetched;
/// a `rope_freqs` of F32 per rotated pair is a few hundred bytes.
const MAX_VECTOR_BYTES: u64 = 1024 * 1024;

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
    let mut vectors: Vec<(ConfigurationVector, Vec<u8>)> = Vec::new();
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
        for (vector, range) in header.configuration_vectors() {
            if range.end > size || range.end - range.start > MAX_VECTOR_BYTES {
                return Err(Error::Config(format!(
                    "{} holds a {} beyond the file or the preflight limit",
                    shard.path,
                    vector.tensor()
                )));
            }
            let (start, end) = (usize::try_from(range.start)?, usize::try_from(range.end)?);
            let value =
                range::fetch(client, token, url.clone(), start, end.saturating_sub(1)).await?;
            fetched += u64::try_from(value.len())?;
            vectors.push((vector, value));
        }
        shards.push((PathBuf::from(&shard.path), header));
    }
    let contract = GgufModel::from_headers(shards, vectors)
        .map_err(libmir::Error::from)
        .and_then(|model| RemoteModelContract::inspect_gguf(&model));
    Ok((contract, fetched))
}
