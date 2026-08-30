use super::NativeRuntime;
use crate::{
    application::Result,
    catalog::{MachineMemory, Removal, SearchResults},
};

impl NativeRuntime {
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

    pub async fn remove_download(&self, repo_id: &str) -> Result<Removal> {
        let key = crate::config::model_key(repo_id)?;
        self.lifecycle.ensure_removable(&key)?;
        let removal = self.catalog.remove(repo_id).await?;
        if removal.removed {
            self.store.deactivate_model(&key)?;
        }
        Ok(removal)
    }
}
