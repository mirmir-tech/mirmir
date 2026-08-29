use std::{
    path::Path,
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use libmir::CancellationToken;
use reqwest::{StatusCode, header};
use tokio::{fs, io::AsyncWriteExt, sync::mpsc};

use super::super::{TransferUpdate, download::send};
use crate::{
    config::Store,
    error::{Error, Result},
};

pub(super) struct Download<'a> {
    store: &'a Store,
    root: &'a Path,
    snapshot: &'a Path,
    total: u64,
    updates: &'a mpsc::Sender<TransferUpdate>,
    cancellation: &'a CancellationToken,
}

impl<'a> Download<'a> {
    pub const fn new(
        store: &'a Store,
        root: &'a Path,
        snapshot: &'a Path,
        total: u64,
        updates: &'a mpsc::Sender<TransferUpdate>,
        cancellation: &'a CancellationToken,
    ) -> Self {
        Self {
            store,
            root,
            snapshot,
            total,
            updates,
            cancellation,
        }
    }

    pub async fn file(
        &self,
        metadata: hf_hub::repository::FileMetadataInfo,
        completed: u64,
    ) -> Result<u64> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let blobs = self.root.join("blobs");
        fs::create_dir_all(&blobs).await?;
        let blob = blobs.join(&metadata.etag);
        let partial = blobs.join(format!("{}.incomplete", metadata.etag));
        if fs::metadata(&blob).await.is_ok_and(|value| value.len() == metadata.file_size) {
            link(&blob, self.snapshot, &metadata.filename).await?;
            return Ok(completed);
        }
        let partial_bytes = fs::metadata(&partial)
            .await
            .map_or(0, |value| value.len().min(metadata.file_size));
        let offset = if partial_bytes < metadata.file_size {
            partial_bytes
        } else {
            0
        };
        let response = response(self.store, &metadata, offset).await?;
        let resumed = offset > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
        let start = if resumed {
            offset
        } else {
            0
        };
        let base = completed.saturating_sub(partial_bytes);
        let mut output = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&partial)
            .await?;
        let mut current = start;
        let mut body = response.bytes_stream();
        let mut last_emit = Instant::now()
            .checked_sub(Duration::from_millis(250))
            .unwrap_or_else(Instant::now);
        loop {
            let chunk = tokio::select! {
                biased;
                () = cancelled(self.cancellation) => return Err(Error::Cancelled),
                chunk = body.next() => chunk,
            };
            let Some(chunk) = chunk else {
                break;
            };
            let chunk = chunk?;
            output.write_all(&chunk).await?;
            current = current.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
            if last_emit.elapsed() >= Duration::from_millis(250) {
                send(
                    self.updates,
                    super::super::TransferPhase::Downloading,
                    base.saturating_add(current),
                    Some(self.total),
                    format!("downloading {}", metadata.filename),
                )
                .await;
                last_emit = Instant::now();
            }
        }
        output.flush().await?;
        if current != metadata.file_size {
            return Err(Error::Config(format!(
                "incomplete response for {}: {current}/{} bytes",
                metadata.filename, metadata.file_size
            )));
        }
        fs::rename(&partial, &blob).await?;
        link(&blob, self.snapshot, &metadata.filename).await?;
        Ok(base.saturating_add(current))
    }
}

async fn response(
    store: &Store,
    metadata: &hf_hub::repository::FileMetadataInfo,
    offset: u64,
) -> Result<reqwest::Response> {
    let location = metadata
        .location
        .as_ref()
        .ok_or_else(|| Error::Config(format!("missing download URL for {}", metadata.filename)))?;
    let client = reqwest::Client::new();
    let mut request = client.get(location);
    if let Some(token) = store.hf_token()? {
        request = request.bearer_auth(token);
    }
    if offset > 0 {
        request = request.header(header::RANGE, format!("bytes={offset}-"));
    }
    let response = request.send().await?;
    if response.status().is_success() {
        return Ok(response);
    }
    Err(Error::Config(format!(
        "download of {} failed with HTTP {}",
        metadata.filename,
        response.status()
    )))
}

async fn link(blob: &Path, snapshot: &Path, filename: &str) -> Result<()> {
    let target = snapshot.join(filename);
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::hard_link(blob, target).await?;
    Ok(())
}

async fn cancelled(token: &CancellationToken) {
    while !token.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
