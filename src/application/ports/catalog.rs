use libmir::CancellationToken;
use tokio::sync::mpsc;

use crate::{
    application::Result,
    catalog::{DownloadedModel, Removal, SearchResults, TransferUpdate},
};

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
        updates: mpsc::Sender<TransferUpdate>,
        cancellation: &CancellationToken,
    ) -> Result<DownloadedModel>;
}
