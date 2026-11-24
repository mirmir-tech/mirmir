use super::NativeRuntime;
use crate::{
    application::Result,
    catalog::{MachineMemory, ModelVariants, Removal, SearchResults},
};

impl NativeRuntime {
    pub async fn search_catalog(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResults> {
        Ok(self.catalog.search(query, limit, self.catalog_memory(), cursor).await?)
    }

    pub async fn catalog_variants(&self, repo_id: &str) -> Result<ModelVariants> {
        Ok(self.catalog.variants(repo_id, self.catalog_memory()).await?)
    }

    fn catalog_memory(&self) -> MachineMemory {
        self.library
            .memory_snapshot()
            .map_or_else(|_| MachineMemory::detect(), |memory| MachineMemory::from_runtime(&memory))
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
