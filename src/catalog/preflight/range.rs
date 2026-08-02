use futures_util::TryStreamExt;

use super::metadata;
use crate::error::{Error, Result};

pub(super) async fn fetch_file(
    client: &reqwest::Client,
    token: Option<&str>,
    base: &reqwest::Url,
    repo_id: &str,
    revision: &str,
    file: &metadata::File,
) -> Result<Vec<u8>> {
    file.validate_size()?;
    let url = super::file_url(base, repo_id, revision, &file.path)?;
    fetch(client, token, url, 0, file.size.saturating_sub(1)).await
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    token: Option<&str>,
    url: reqwest::Url,
    start: usize,
    end: usize,
) -> Result<Vec<u8>> {
    let mut request = client
        .get(url)
        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
        .header(reqwest::header::USER_AGENT, concat!("mirmir/", env!("CARGO_PKG_VERSION")));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(Error::Config(format!(
            "Hub ignored byte range {start}-{end}: HTTP {}",
            response.status()
        )));
    }
    validate_content_range(response.headers(), start, end)?;
    let expected = end
        .checked_sub(start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| Error::Config("invalid byte range".into()))?;
    read_exact_response(response, expected).await
}

fn validate_content_range(
    headers: &reqwest::header::HeaderMap,
    start: usize,
    end: usize,
) -> Result<()> {
    let value = headers
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| Error::Config("Hub omitted Content-Range".into()))?;
    let (range, total) = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split_once('/'))
        .ok_or_else(|| Error::Config(format!("invalid Content-Range `{value}`")))?;
    let expected = format!("{start}-{end}");
    let Ok(total) = total.parse::<usize>() else {
        return Err(Error::Config(format!("invalid Content-Range `{value}`")));
    };
    if range != expected || total <= end {
        return Err(Error::Config(format!(
            "Hub returned Content-Range `{value}` for bytes {start}-{end}"
        )));
    }
    Ok(())
}

async fn read_exact_response(response: reqwest::Response, expected: usize) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(expected);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        if body.len().saturating_add(chunk.len()) > expected {
            return Err(Error::Config("Hub returned more bytes than the requested range".into()));
        }
        body.extend_from_slice(&chunk);
    }
    if body.len() != expected {
        return Err(Error::Config(format!(
            "Hub returned {} bytes for a {expected}-byte range",
            body.len()
        )));
    }
    Ok(body)
}
