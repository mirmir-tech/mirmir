use libmir::{EmbeddingRequest, RerankRequest};
use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

impl RuntimeService {
    pub(crate) async fn embed_request(
        &self,
        request: proto::EmbedRequest,
    ) -> Result<proto::EmbedResponse, Status> {
        let service = self.clone();
        super::status::internal(
            tokio::task::spawn_blocking(move || embed(&service, request)).await,
        )?
    }

    pub(crate) async fn rerank_request(
        &self,
        request: proto::RerankRequest,
    ) -> Result<proto::RerankResponse, Status> {
        let service = self.clone();
        super::status::internal(
            tokio::task::spawn_blocking(move || rerank(&service, request)).await,
        )?
    }
}

pub(super) fn embed(
    service: &RuntimeService,
    request: proto::EmbedRequest,
) -> Result<proto::EmbedResponse, Status> {
    if request.model.trim().is_empty() || request.inputs.is_empty() {
        return Err(Status::invalid_argument("model and at least one input are required"));
    }
    let output = service
        .application
        .embed(
            &request.model,
            EmbeddingRequest {
                inputs: request.inputs,
                dimensions: super::status::invalid(
                    request.dimensions.map(usize::try_from).transpose(),
                )?,
                prompt_name: request.prompt_name,
            },
        )
        .map_err(super::status::application)?;
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
    let output = service
        .application
        .rerank(
            &request.model,
            RerankRequest {
                query: request.query,
                documents: request.documents,
                max_length: super::status::invalid(
                    request.max_length.map(usize::try_from).transpose(),
                )?,
                raw_scores: request.raw_scores,
            },
        )
        .map_err(super::status::application)?;
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
