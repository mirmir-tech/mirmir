use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::RuntimeService;
use crate::{
    catalog::{SearchResults, TransferUpdate},
    rpc::proto,
};

pub fn response(results: SearchResults) -> proto::SearchModelsResponse {
    proto::SearchModelsResponse {
        models: results.models.into_iter().map(model).collect(),
        total_memory_bytes: results.memory.total,
        available_memory_bytes: results.memory.available,
        memory_source: results.memory.source.to_owned(),
        next_cursor: results.next_cursor,
    }
}

fn model(model: crate::catalog::CatalogModel) -> proto::CatalogModel {
    proto::CatalogModel {
        id: model.id,
        downloads: model.downloads,
        likes: model.likes,
        gated: model.gated,
        architecture: model.architecture,
        compatibility: model.compatibility.to_owned(),
        memory_fit: model.memory_fit.to_owned(),
        estimated_weight_bytes: model.weight_bytes,
        estimated_required_bytes: model.required_bytes,
        budget_bytes: model.budget_bytes,
        confidence: model.confidence.to_owned(),
        reason: model.reason,
        downloaded: model.downloaded,
        local_source: model.local_source.to_owned(),
    }
}

pub async fn search(
    service: &RuntimeService,
    request: proto::SearchModelsRequest,
) -> Result<proto::SearchModelsResponse, Status> {
    if request.query.trim().is_empty() {
        return Err(Status::invalid_argument("search query cannot be empty"));
    }
    let limit = if request.limit == 0 {
        20
    } else {
        request.limit
    };
    let memory = service.library.memory_snapshot().map_or_else(
        |_| crate::catalog::MachineMemory::detect(),
        |memory| crate::catalog::MachineMemory::from_runtime(&memory),
    );
    let results = service
        .catalog
        .search(
            request.query.trim(),
            usize::try_from(limit).unwrap_or(20),
            memory,
            request.cursor.as_deref(),
        )
        .await
        .map_err(|error| Status::unavailable(error.to_string()))?;
    Ok(response(results))
}

pub fn pull_stream(
    service: RuntimeService,
    request: proto::PullModelRequest,
) -> Result<ReceiverStream<Result<proto::ModelTransferEvent, Status>>, Status> {
    if request.repo_id.trim().is_empty() {
        return Err(Status::invalid_argument("repository id cannot be empty"));
    }
    let (sender, receiver) = mpsc::channel(64);
    drop(tokio::spawn(pull(service, request, sender)));
    Ok(ReceiverStream::new(receiver))
}

pub async fn remove(
    service: &RuntimeService,
    repo_id: String,
) -> Result<proto::RemoveModelResponse, Status> {
    let key = crate::config::model_key(&repo_id)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if service
        .models
        .lock()
        .map_err(|_| Status::internal("model registry lock poisoned"))?
        .contains_key(&key)
    {
        return Err(Status::failed_precondition("unload the model before removing it"));
    }
    if service
        .loading
        .lock()
        .map_err(|_| Status::internal("model lifecycle lock poisoned"))?
        .contains(&key)
    {
        return Err(Status::failed_precondition("wait for model loading to finish"));
    }
    let removal = service
        .catalog
        .remove(&repo_id)
        .await
        .map_err(|error| Status::failed_precondition(error.to_string()))?;
    if removal.removed {
        service
            .store
            .deactivate_model(&key)
            .map_err(|error| Status::internal(error.to_string()))?;
    }
    Ok(proto::RemoveModelResponse {
        removed: removal.removed,
        freed_bytes: removal.freed_bytes,
    })
}

pub fn transfer(
    operation_id: &str,
    repo_id: &str,
    update: TransferUpdate,
) -> proto::ModelTransferEvent {
    proto::ModelTransferEvent {
        repo_id: repo_id.to_owned(),
        phase: update.phase.to_owned(),
        downloaded_bytes: update.downloaded_bytes,
        total_bytes: update.total_bytes,
        path: None,
        message: update.message,
        operation_id: operation_id.to_owned(),
    }
}

async fn pull(
    service: RuntimeService,
    request: proto::PullModelRequest,
    output: mpsc::Sender<Result<proto::ModelTransferEvent, tonic::Status>>,
) {
    let repo_id = request.repo_id;
    let operation = service.activity.begin("pull", &repo_id, None);
    let (updates, mut receiver) = mpsc::channel::<TransferUpdate>(64);
    let forward = output.clone();
    let forwarded_repo = repo_id.clone();
    let forwarded_operation = operation.clone();
    let task = tokio::spawn(async move {
        while let Some(update) = receiver.recv().await {
            forwarded_operation.progress(
                update.phase,
                &update.message,
                Some(update.downloaded_bytes),
                update.total_bytes,
            );
            if forward
                .send(Ok(transfer(forwarded_operation.id(), &forwarded_repo, update)))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    match service.catalog.pull(&repo_id, request.revision.as_deref(), updates).await {
        Ok(model) => {
            drop(task.await);
            operation.finish("completed", "model is available");
            let event = proto::ModelTransferEvent {
                repo_id,
                phase: "available".to_owned(),
                downloaded_bytes: 0,
                total_bytes: None,
                path: Some(model.path.display().to_string()),
                message: "model is available".to_owned(),
                operation_id: operation.id().to_owned(),
            };
            drop(output.send(Ok(event)).await);
        },
        Err(error) => {
            drop(task.await);
            operation.finish("failed", &error.to_string());
            drop(output.send(Err(tonic::Status::unavailable(error.to_string()))).await);
        },
    }
}
