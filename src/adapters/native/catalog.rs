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

    pub(super) fn catalog_memory(&self) -> MachineMemory {
        self.library
            .memory_snapshot()
            .map_or_else(|_| MachineMemory::detect(), |memory| MachineMemory::from_runtime(&memory))
    }

    /// Removes `repo_id`, a model or a whole repository in any spelling,
    /// unless a model it takes is loaded or loading, by its key or by a path
    /// within a cache the removal may delete; none loads meanwhile, the
    /// removal deletes no other cache, and none of what it took is restored
    /// as active afterwards.
    #[expect(
        clippy::significant_drop_tightening,
        reason = "the prepared removal's exclusion moves into `run`, which holds it while deleting"
    )]
    pub async fn remove_download(&self, repo_id: &str) -> Result<Removal> {
        // Scoped once nothing of the repository transfers, so the models
        // held off are the ones the removal deletes.
        let prepared = self.catalog.prepare_removal(repo_id).await?;
        let scope = prepared.scope().clone();
        let _removal = self.lifecycle.begin_removal(&scope)?;
        let removal = prepared.run().await?;
        if removal.removed {
            self.store.deactivate_models_of(&scope)?;
        }
        Ok(removal)
    }
}
