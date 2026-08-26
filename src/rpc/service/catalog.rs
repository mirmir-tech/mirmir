use libmir::CancellationToken;
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
        model_class: model.model_class,
        compatibility: model.compatibility.to_owned(),
        memory_fit: model.memory_fit.to_owned(),
        estimated_weight_bytes: model.weight_bytes,
        estimated_required_bytes: model.required_bytes,
        budget_bytes: model.budget_bytes,
        confidence: model.confidence.to_owned(),
        reason: model.reason,
        downloaded: model.downloaded,
        local_source: model.local_source.to_owned(),
        library: model.library,
        ecosystem: model.ecosystem,
        container: model.container,
        encoding: model.encoding,
        metal_compatibility: model.metal_compatibility,
        cuda_compatibility: model.cuda_compatibility,
        preflight_bytes: model.preflight_bytes,
        preflight_error: model.preflight_error,
        tool_use: model.features.tool_use,
        thinking: model.features.thinking,
        vision: model.features.vision,
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
    let results = super::status::unavailable(
        service
            .coordinator
            .search_catalog(
                request.query.trim(),
                usize::try_from(limit).unwrap_or(20),
                request.cursor.as_deref(),
            )
            .await,
    )?;
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
    let removal = service
        .coordinator
        .remove_download(&repo_id)
        .await
        .map_err(|error| super::models::load_error(&error))?;
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
    let cancellation = CancellationToken::new();
    let operation = service.activity.enqueue("pull", &repo_id, cancellation.clone());
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
    match service
        .coordinator
        .pull_model(&repo_id, request.revision.as_deref(), updates, &cancellation)
        .await
    {
        Ok(model) => {
            drop(task.await);
            let message = model.load_unavailable_reason.as_ref().map_or_else(
                || "model is available".to_owned(),
                |reason| format!("model downloaded; loading is unavailable: {reason}"),
            );
            operation.finish("completed", &message);
            let event = proto::ModelTransferEvent {
                repo_id,
                phase: "available".to_owned(),
                downloaded_bytes: 0,
                total_bytes: None,
                path: Some(model.config.path.display().to_string()),
                message,
                operation_id: operation.id().to_owned(),
            };
            drop(output.send(Ok(event)).await);
        },
        Err(crate::application::Error::Configuration(crate::error::Error::Cancelled)) => {
            drop(task.await);
            operation.finish("cancelled", "download stopped; partial files kept for resume");
            drop(output.send(Err(tonic::Status::cancelled("download stopped"))).await);
        },
        Err(error) => {
            drop(task.await);
            operation.finish("failed", &error.to_string());
            drop(output.send(Err(tonic::Status::unavailable(error.to_string()))).await);
        },
    }
}
