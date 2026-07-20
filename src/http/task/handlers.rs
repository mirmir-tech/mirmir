use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use tonic::Request;

use super::types::{
    EmbeddingData, EmbeddingsRequest, EmbeddingsResponse, RerankData, RerankDocument,
    RerankRequest, RerankResponse, TaskUsage,
};
use crate::{
    http::{ApiState, error::ApiError},
    rpc::{proto, proto::runtime_server::Runtime},
};

pub async fn embeddings(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<EmbeddingsRequest>, JsonRejection>,
) -> Result<Json<EmbeddingsResponse>, ApiError> {
    state.authorize(&headers)?;
    let request = payload.map_err(|error| ApiError::bad_request(error.body_text()))?.0;
    let model = request.model.clone();
    let response = state
        .service
        .embed(Request::new(request.into_proto()?))
        .await
        .map_err(ApiError::from_status)?
        .into_inner();
    let prompt_tokens = response.prompt_tokens;
    Ok(Json(EmbeddingsResponse {
        object: "list",
        data: response
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingData {
                object: "embedding",
                embedding: embedding.values,
                index,
            })
            .collect(),
        model,
        usage: TaskUsage {
            prompt_tokens,
            total_tokens: prompt_tokens,
        },
    }))
}

pub async fn rerank(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<RerankRequest>, JsonRejection>,
) -> Result<Json<RerankResponse>, ApiError> {
    state.authorize(&headers)?;
    let request = payload.map_err(|error| ApiError::bad_request(error.body_text()))?.0;
    let return_documents = request.return_documents;
    let response = state
        .service
        .rerank(Request::new(proto::RerankRequest {
            model: request.model,
            query: request.query,
            documents: request.documents,
            max_length: request.max_length,
            raw_scores: request.raw_scores,
        }))
        .await
        .map_err(ApiError::from_status)?
        .into_inner();
    let prompt_tokens = response.prompt_tokens;
    Ok(Json(RerankResponse {
        results: response
            .results
            .into_iter()
            .map(|result| RerankData {
                index: result.index,
                relevance_score: result.score,
                document: return_documents.then_some(RerankDocument { text: result.document }),
            })
            .collect(),
        usage: TaskUsage {
            prompt_tokens,
            total_tokens: prompt_tokens,
        },
    }))
}
