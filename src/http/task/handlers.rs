use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};

use super::types::{
    EmbeddingData, EmbeddingsRequest, EmbeddingsResponse, RerankData, RerankDocument,
    RerankRequest, RerankResponse, TaskUsage,
};
use crate::http::{ApiState, error::ApiError};

pub async fn embeddings(
    State(state): State<ApiState>,
    headers: HeaderMap,
    payload: Result<Json<EmbeddingsRequest>, JsonRejection>,
) -> Result<Json<EmbeddingsResponse>, ApiError> {
    state.authorize(&headers)?;
    let request = match payload {
        Ok(request) => request.0,
        Err(error) => return Err(ApiError::bad_request(error.body_text())),
    };
    let (model, request) = request.into_application()?;
    let response = state
        .application()
        .embed(&model, request)
        .map_err(|error| ApiError::from_application(&error))?;
    let prompt_tokens = u64::try_from(response.prompt_tokens).unwrap_or(u64::MAX);
    Ok(Json(EmbeddingsResponse {
        object: "list",
        data: response
            .embeddings
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| EmbeddingData { object: "embedding", embedding, index })
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
    let request = match payload {
        Ok(request) => request.0,
        Err(error) => return Err(ApiError::bad_request(error.body_text())),
    };
    if request.model.trim().is_empty() || request.query.is_empty() || request.documents.is_empty() {
        return Err(ApiError::bad_request("model, query, and at least one document are required"));
    }
    let return_documents = request.return_documents;
    let top_n = request.top_n.unwrap_or(request.documents.len());
    let max_length = request
        .max_length
        .map(usize::try_from)
        .transpose()
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    let response = state
        .application()
        .rerank(
            &request.model,
            libmir::RerankRequest {
                query: request.query,
                documents: request.documents,
                max_length,
                raw_scores: request.raw_scores,
            },
        )
        .map_err(|error| ApiError::from_application(&error))?;
    let prompt_tokens = u64::try_from(response.prompt_tokens).unwrap_or(u64::MAX);
    Ok(Json(RerankResponse {
        results: response
            .results
            .into_iter()
            .take(top_n)
            .map(|result| RerankData {
                index: u64::try_from(result.index).unwrap_or(u64::MAX),
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
