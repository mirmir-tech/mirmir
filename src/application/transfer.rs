use libmir::CancellationToken;
use tokio::sync::mpsc;

use super::{Application, Operation, Result};
use crate::catalog::{DownloadedModel, TransferUpdate};

pub struct TransferSession {
    repo_id: String,
    revision: Option<String>,
    operation: Operation,
    cancellation: CancellationToken,
}

impl Application {
    pub fn start_pull(&self, repo_id: &str, revision: Option<String>) -> TransferSession {
        let cancellation = CancellationToken::new();
        TransferSession {
            repo_id: repo_id.to_owned(),
            revision,
            operation: self.activity.enqueue("pull", repo_id, cancellation.clone()),
            cancellation,
        }
    }

    pub async fn pull(
        &self,
        session: TransferSession,
        output: mpsc::Sender<TransferUpdate>,
    ) -> Result<DownloadedModel> {
        let (updates, mut receiver) = mpsc::channel::<TransferUpdate>(64);
        let operation = session.operation.clone();
        let cancellation = session.cancellation.clone();
        let forward = tokio::spawn(async move {
            while let Some(update) = receiver.recv().await {
                operation.progress(
                    update.phase,
                    &update.message,
                    Some(update.downloaded_bytes),
                    update.total_bytes,
                );
                if output.send(update).await.is_err() {
                    cancellation.cancel();
                    break;
                }
            }
        });
        let result = self
            .catalog
            .pull(&session.repo_id, session.revision.as_deref(), updates, &session.cancellation)
            .await;
        forward.await.map_err(|error| super::Error::Infrastructure(error.to_string()))?;
        match &result {
            Ok(model) => session.operation.finish("completed", &available_message(model)),
            Err(super::Error::Cancelled) => session
                .operation
                .finish("cancelled", "download stopped; partial files kept for resume"),
            Err(error) => session.operation.finish("failed", &error.to_string()),
        }
        result
    }
}

impl TransferSession {
    #[must_use]
    pub fn operation_id(&self) -> &str {
        self.operation.id()
    }
}

#[must_use]
pub fn available_message(model: &DownloadedModel) -> String {
    model.load_unavailable_reason.as_ref().map_or_else(
        || "model is available".to_owned(),
        |reason| format!("model downloaded; loading is unavailable: {reason}"),
    )
}
