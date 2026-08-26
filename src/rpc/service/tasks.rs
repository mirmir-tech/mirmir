use libmir::{EmbeddingRequest, RerankRequest};
use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

pub(super) async fn embed_rpc(
    service: RuntimeService,
    request: proto::EmbedRequest,
) -> Result<proto::EmbedResponse, Status> {
    super::status::internal(tokio::task::spawn_blocking(move || embed(&service, request)).await)?
}

pub(super) async fn rerank_rpc(
    service: RuntimeService,
    request: proto::RerankRequest,
) -> Result<proto::RerankResponse, Status> {
    super::status::internal(tokio::task::spawn_blocking(move || rerank(&service, request)).await)?
}

pub(super) fn embed(
    service: &RuntimeService,
    request: proto::EmbedRequest,
) -> Result<proto::EmbedResponse, Status> {
    if request.model.trim().is_empty() || request.inputs.is_empty() {
        return Err(Status::invalid_argument("model and at least one input are required"));
    }
    let model = load(service, &request.model)?;
    let output = model.embed(EmbeddingRequest {
        inputs: request.inputs,
        dimensions: super::status::invalid(request.dimensions.map(usize::try_from).transpose())?,
        prompt_name: request.prompt_name,
    });
    let output = match output {
        Ok(output) => output,
        Err(error) => return Err(task_error(&error)),
    };
    Ok(proto::EmbedResponse {
        embeddings: output
            .embeddings
            .into_iter()
            .map(|values| proto::EmbeddingVector { values })
            .collect(),
        prompt_tokens: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
    })
}

pub(super) fn rerank(
    service: &RuntimeService,
    request: proto::RerankRequest,
) -> Result<proto::RerankResponse, Status> {
    if request.model.trim().is_empty() || request.query.is_empty() || request.documents.is_empty() {
        return Err(Status::invalid_argument(
            "model, query, and at least one document are required",
        ));
    }
    let model = load(service, &request.model)?;
    let output = model.rerank(RerankRequest {
        query: request.query,
        documents: request.documents,
        max_length: super::status::invalid(request.max_length.map(usize::try_from).transpose())?,
        raw_scores: request.raw_scores,
    });
    let output = match output {
        Ok(output) => output,
        Err(error) => return Err(task_error(&error)),
    };
    Ok(proto::RerankResponse {
        results: output
            .results
            .into_iter()
            .map(|result| proto::RerankResult {
                index: u64::try_from(result.index).unwrap_or(u64::MAX),
                score: result.score,
                document: result.document,
            })
            .collect(),
        prompt_tokens: u64::try_from(output.prompt_tokens).unwrap_or(u64::MAX),
    })
}

fn load(service: &RuntimeService, selector: &str) -> Result<libmir::Model, Status> {
    let mut ignored = |_progress| {};
    service
        .coordinator
        .load_model(selector, false, &mut ignored)
        .map(|entry| entry.model)
        .map_err(|error| super::models::load_error(&error))
}

fn task_error(error: &libmir::Error) -> Status {
    match error {
        libmir::Error::EmptyPrompt
        | libmir::Error::TaskMismatch { .. }
        | libmir::Error::Model(_)
        | libmir::Error::Context { .. } => Status::invalid_argument(error.to_string()),
        _ => Status::internal(error.to_string()),
    }
}
