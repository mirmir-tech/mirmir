use super::NativeRuntime;
use crate::{
    application::Result,
    catalog::{MachineMemory, ModelVariants, Removal, SearchResults},
    config::HubReference,
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

    pub(super) fn catalog_memory(&self) -> MachineMemory {
        self.library
            .memory_snapshot()
            .map_or_else(|_| MachineMemory::detect(), |memory| MachineMemory::from_runtime(&memory))
    }

    /// Removes `repo_id`, a model or a whole repository in any spelling,
    /// unless a model of it is loaded or loading; none loads meanwhile, and
    /// none of it is restored as active afterwards.
    pub async fn remove_download(&self, repo_id: &str) -> Result<Removal> {
        let asked = repo_id.parse::<HubReference>()?;
        let _removal = self.lifecycle.begin_removal(&asked)?;
        let removal = self.catalog.remove(repo_id).await?;
        if removal.removed {
            self.store.deactivate_models_of(&asked)?;
        }
        Ok(removal)
    }
}
