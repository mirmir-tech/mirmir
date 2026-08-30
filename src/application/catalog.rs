use super::{ActivityKind, ActivityOutcome, Application, Result};
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
        let operation = self.activity.begin(ActivityKind::Remove, repo_id, None);
        let result = self.catalog.remove(repo_id).await;
        operation.finish(
            if result.is_ok() {
                ActivityOutcome::Completed
            } else {
                ActivityOutcome::Failed
            },
            "model removal finished",
        );
        result
    }
}
