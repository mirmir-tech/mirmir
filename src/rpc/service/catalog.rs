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
    let results = service
        .application
        .search_catalog(
            request.query.trim(),
            usize::try_from(limit).unwrap_or(20),
            request.cursor.as_deref(),
        )
        .await
        .map_err(super::status::application)?;
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
        .application
        .remove_download(&repo_id)
        .await
        .map_err(super::status::application)?;
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
        phase: update.phase.as_str().to_owned(),
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
    let session = service.application.start_pull(&repo_id, request.revision.clone());
    let operation_id = session.operation_id().to_owned();
    let (updates, mut receiver) = mpsc::channel::<TransferUpdate>(64);
    let forward = output.clone();
    let forwarded_repo = repo_id.clone();
    let forwarded_operation = operation_id.clone();
    let task = tokio::spawn(async move {
        while let Some(update) = receiver.recv().await {
            if forward
                .send(Ok(transfer(&forwarded_operation, &forwarded_repo, update)))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    match service.application.pull(session, updates).await {
        Ok(model) => {
            drop(task.await);
            let message = crate::application::available_message(&model);
            let event = proto::ModelTransferEvent {
                repo_id,
                phase: "available".to_owned(),
                downloaded_bytes: 0,
                total_bytes: None,
                path: Some(model.config.path.display().to_string()),
                message,
                operation_id,
            };
            drop(output.send(Ok(event)).await);
        },
        Err(crate::application::Error::Cancelled) => {
            drop(task.await);
            drop(output.send(Err(tonic::Status::cancelled("download stopped"))).await);
        },
        Err(error) => {
            drop(task.await);
            drop(output.send(Err(super::status::application(error))).await);
        },
    }
}
