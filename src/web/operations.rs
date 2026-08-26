use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use super::{security_headers, session::WebError, types::CatalogResults};
use crate::http::ApiState;

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
    let response = state
        .application()
        .search_catalog(
            &query.query,
            usize::try_from(query.limit.unwrap_or(20)).unwrap_or(usize::MAX),
            query.cursor.as_deref(),
        )
        .await
        .map_err(WebError::application)?;
    Ok((security_headers(), Json(CatalogResults::from(response))).into_response())
}

pub async fn pull(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<PullRequest>,
) -> Result<Response, WebError> {
    state.sessions().authorize_mutation(&headers)?;
    if request.repo_id.trim().is_empty() {
        return Err(WebError::invalid("repository id cannot be empty"));
    }
    let application = state.application().clone();
    let session = application.start_pull(&request.repo_id, request.revision);
    let (sender, mut events) = tokio::sync::mpsc::channel(32);
    drop(tokio::spawn(async move {
        let drain = tokio::spawn(async move { while events.recv().await.is_some() {} });
        drop(application.pull(session, sender).await);
        drop(drain.await);
    }));
    Ok(accepted("pull"))
}

fn accepted(operation: &'static str) -> Response {
    (security_headers(), Json(Accepted { accepted: true, operation })).into_response()
}
