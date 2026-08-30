use tonic::Status;

use super::{RuntimeService, catalog};
use crate::rpc::proto;

impl RuntimeService {
    pub(crate) fn fail_startup(&self, detail: impl Into<String>) {
        self.application.fail_startup(detail);
    }

    pub(super) async fn remove_request(
        &self,
        repo_id: String,
    ) -> Result<proto::RemoveModelResponse, Status> {
        catalog::remove(self, repo_id).await
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        application::ActivityStage,
        config::{AppConfig, Paths, Store},
    };

    #[tokio::test]
    async fn load_progress_reaches_activity_subscribers() {
        let root = std::env::temp_dir().join(format!("mirmir-dashboard-{}", std::process::id()));
        let paths =
            Paths::from_roots(root.join("config"), root.join("state"), &root.join("runtime"));
        let service = RuntimeService::new(&AppConfig::default(), Store::new(paths));
        let mut activity = service.application.activity_updates();
        let request = proto::LoadModelRequest {
            selector: "missing-model".to_owned(),
            ..Default::default()
        };
        let (sender, _receiver) = mpsc::channel(8);
        let loading = service.clone();
        tokio::task::spawn_blocking(move || {
            super::super::models::stream_load(&loading, &request, &sender);
        })
        .await
        .expect("load task should finish");

        let events = tokio::time::timeout(std::time::Duration::from_secs(2), async move {
            let mut operation_ids = std::collections::HashSet::new();
            let mut statuses = Vec::new();
            while let Ok(event) = activity.recv().await {
                operation_ids.insert(event.operation_id.clone());
                statuses.push(event.status);
                if event.status.is_terminal() {
                    break;
                }
            }
            (operation_ids, statuses)
        })
        .await
        .expect("activity stream should reach a terminal event");
        let (operation_ids, statuses) = events;
        assert_eq!(operation_ids.len(), 1, "load must own exactly one application operation");
        assert!(
            statuses
                .iter()
                .any(|status| status.stage_str() == ActivityStage::Resolving.as_str())
        );
        assert_eq!(statuses.last().map(|status| status.state_str()), Some("failed"));
    }
}
