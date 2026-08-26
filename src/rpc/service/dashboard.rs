use tokio::sync::watch;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::{RuntimeService, activity, catalog};
use crate::{application::StartupSnapshot, rpc::proto};

impl RuntimeService {
    pub(crate) fn startup_snapshot(&self) -> StartupSnapshot {
        self.startup.snapshot()
    }

    pub(crate) fn watch_startup(&self) -> watch::Receiver<StartupSnapshot> {
        self.startup.subscribe()
    }

    pub(crate) fn fail_startup(&self, detail: impl Into<String>) {
        self.startup.failed(detail);
    }

    pub(crate) fn activity_history(&self) -> Vec<proto::ActivityEvent> {
        self.activity.history().into_iter().map(activity::event).collect()
    }

    pub(crate) fn watch_activity_updates(
        &self,
    ) -> ReceiverStream<Result<proto::ActivityEvent, Status>> {
        activity::watch(&self.activity, false)
    }

    pub(super) async fn remove_with_activity(
        &self,
        repo_id: String,
    ) -> Result<proto::RemoveModelResponse, Status> {
        let operation = self.activity.begin("remove", &repo_id, None);
        match catalog::remove(self, repo_id).await {
            Ok(response) => {
                operation.finish("completed", "model files removed");
                Ok(response)
            },
            Err(error) => {
                operation.finish("failed", error.message());
                Err(error)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use tokio::sync::mpsc;

    use super::*;
    use crate::config::{AppConfig, Paths, Store};

    #[tokio::test]
    async fn load_progress_reaches_activity_subscribers() {
        let root = std::env::temp_dir().join(format!("mirmir-dashboard-{}", std::process::id()));
        let paths =
            Paths::from_roots(root.join("config"), root.join("state"), &root.join("runtime"));
        let service = RuntimeService::new(&AppConfig::default(), Store::new(paths));
        let mut activity = service.watch_activity_updates();
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

        let stages = tokio::time::timeout(std::time::Duration::from_secs(2), async move {
            let mut stages = Vec::new();
            while let Some(event) = activity.next().await {
                let event = event.expect("activity event should be valid");
                stages.push(event.stage.clone());
                if event.state == "failed" {
                    break;
                }
            }
            stages
        })
        .await
        .expect("activity stream should reach a terminal event");
        assert!(stages.iter().any(|stage| stage == "resolving"));
        assert_eq!(stages.last().map(String::as_str), Some("failed"));
    }
}
