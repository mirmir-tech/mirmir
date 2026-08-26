use std::{convert::Infallible, time::Duration};

use axum::{
    extract::State,
    http::HeaderMap,
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
};
use futures_util::{StreamExt, stream};

use super::{security_headers, session::WebError, types::Activity};
use crate::http::ApiState;

pub async fn activity(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Response, WebError> {
    state.sessions().authenticate(&headers)?;
    let mut shutdown = state.shutdown();
    let application = state.application();
    let history = stream::iter(application.activity_history());
    let updates = tokio_stream::wrappers::BroadcastStream::new(application.activity_updates())
        .filter_map(|result| async move { result.ok() });
    let events = history
        .chain(updates)
        .map(|event| {
            let (kind, data) = (
                "activity",
                serde_json::to_string(&Activity::from(event))
                    .unwrap_or_else(|error| error_event(&error.to_string())),
            );
            Ok::<_, Infallible>(Event::default().event(kind).data(data))
        })
        .take_until(async move {
            drop(shutdown.changed().await);
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
