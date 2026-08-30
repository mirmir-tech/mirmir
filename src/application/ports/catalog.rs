use libmir::CancellationToken;
use tokio::sync::mpsc;

use crate::{
    application::Result,
    catalog::{DownloadedModel, Removal, SearchResults},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPhase {
    Queued,
    Checking,
    Resolving,
    Downloading,
    Validating,
    Available,
}

impl TransferPhase {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Checking => "checking",
            Self::Resolving => "resolving",
            Self::Downloading => "downloading",
            Self::Validating => "validating",
            Self::Available => "available",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub phase: TransferPhase,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub message: String,
}

#[tonic::async_trait]
pub trait CatalogPort: Send + Sync {
    async fn search(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResults>;

    async fn remove(&self, repo_id: &str) -> Result<Removal>;

    async fn pull(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        updates: mpsc::Sender<TransferProgress>,
        cancellation: &CancellationToken,
    ) -> Result<DownloadedModel>;
}
