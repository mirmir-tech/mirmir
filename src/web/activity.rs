use std::{convert::Infallible, time::Duration};

use axum::{
    extract::State,
    http::HeaderMap,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::StreamExt;
use tonic::Request;

use super::{security_headers, session::WebError, types::Activity};
use crate::{
    http::ApiState,
    rpc::{proto, proto::runtime_server::Runtime},
};

pub async fn activity(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let events = state
        .service()
        .watch_activity(Request::new(proto::WatchActivityRequest { include_history: true }))
        .await
        .map_err(WebError::from_status)?
        .into_inner()
        .map(|result| {
            let (kind, data) = match result {
                Ok(event) => (
                    "activity",
                    serde_json::to_string(&Activity::from(event))
                        .unwrap_or_else(|error| error_event(&error.to_string())),
                ),
                Err(error) => ("error", error_event(error.message())),
            };
            Ok::<_, Infallible>(Event::default().event(kind).data(data))
        });
    let mut response = Sse::new(events)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("keep-alive"))
        .into_response();
    response.headers_mut().extend(security_headers());
    Ok(response)
}

fn error_event(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}
