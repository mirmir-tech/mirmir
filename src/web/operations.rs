use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tonic::Request;

use super::{security_headers, session::WebError, types::CatalogResults};
use crate::{
    http::ApiState,
    rpc::{proto, proto::runtime_server::Runtime},
};

#[derive(Deserialize)]
pub struct SearchQuery {
    query: String,
    limit: Option<u32>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct PullRequest {
    repo_id: String,
    revision: Option<String>,
}

#[derive(Serialize)]
struct Accepted {
    accepted: bool,
    operation: &'static str,
}

pub async fn search(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let response = super::result::status(
        state
            .service()
            .search_models(Request::new(proto::SearchModelsRequest {
                query: query.query,
                limit: query.limit.unwrap_or(20),
                cursor: query.cursor,
            }))
            .await,
    )?
    .into_inner();
    Ok((security_headers(), Json(CatalogResults::from(response))).into_response())
}

pub async fn pull(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<PullRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    let mut events = super::result::status(
        state
            .service()
            .pull_model(Request::new(proto::PullModelRequest {
                repo_id: request.repo_id,
                revision: request.revision,
            }))
            .await,
    )?
    .into_inner();
    drop(tokio::spawn(async move { while events.next().await.is_some() {} }));
    Ok(accepted("pull"))
}

fn accepted(operation: &'static str) -> Response {
    (security_headers(), Json(Accepted { accepted: true, operation })).into_response()
}
