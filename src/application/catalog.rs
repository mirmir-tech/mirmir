use libmir::CancellationToken;
use tokio::sync::mpsc;

use super::{Error, Result, RuntimeCoordinator};
use crate::catalog::{DownloadedModel, MachineMemory, Removal, SearchResults, TransferUpdate};

impl RuntimeCoordinator {
    pub async fn search_catalog(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResults> {
        let memory = self.library.memory_snapshot().map_or_else(
            |_| MachineMemory::detect(),
            |memory| MachineMemory::from_runtime(&memory),
        );
        Ok(self.catalog.search(query, limit, memory, cursor).await?)
    }

    pub async fn pull_model(
        &self,
        repo_id: &str,
        revision: Option<&str>,
        updates: mpsc::Sender<TransferUpdate>,
        cancellation: &CancellationToken,
    ) -> Result<DownloadedModel> {
        Ok(self.catalog.pull(repo_id, revision, updates, cancellation).await?)
    }

    pub async fn remove_download(&self, repo_id: &str) -> Result<Removal> {
        let key = crate::config::model_key(repo_id)?;
        let loaded = {
            let models = self.models.lock().map_err(|_| Error::StatePoisoned("model registry"))?;
            models.contains_key(&key)
        };
        if loaded {
            return Err(Error::ModelLoaded(key));
        }
        let is_loading = {
            let loading =
                self.loading.lock().map_err(|_| Error::StatePoisoned("model lifecycle"))?;
            loading.contains(&key)
        };
        if is_loading {
            return Err(Error::ModelAlreadyLoading(key));
        }
        let removal = self.catalog.remove(repo_id).await?;
        if removal.removed {
            self.store.deactivate_model(&key)?;
        }
        Ok(removal)
    }

    pub async fn test_hf_token(&self) -> Result<String> {
        Ok(self.catalog.test_hf_token().await?)
    }
}
