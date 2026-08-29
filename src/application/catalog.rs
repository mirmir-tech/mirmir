use super::{Application, Result};
use crate::catalog::{Removal, SearchResults};

impl Application {
    pub async fn search_catalog(
        &self,
        query: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<SearchResults> {
        self.catalog.search(query, limit, cursor).await
    }

    pub async fn remove_download(&self, repo_id: &str) -> Result<Removal> {
        let operation = self.activity.begin("remove", repo_id, None);
        let result = self.catalog.remove(repo_id).await;
        operation.finish(
            if result.is_ok() {
                "completed"
            } else {
                "failed"
            },
            "model removal finished",
        );
        result
    }
}
