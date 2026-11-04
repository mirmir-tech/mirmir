use std::{sync::mpsc::channel, time::Duration};

use axum::{Router, http::StatusCode, routing::post};
use serde_json::json;
use tokio::{net::TcpListener, sync::watch};
use tower_http::timeout::TimeoutLayer;

use super::{Outcome, WAIT};
use crate::{
    config::{AppConfig, Paths, Store},
    http::{
        ApiState,
        decision::{Decisions, decide, types::DecideResponse, worker::Worker},
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_timeout_discards_queued_work_before_the_next_live_request() -> Outcome<()> {
    let (started, starts) = channel();
    let (gate, release) = channel();
    let worker = Worker::spawn(4, move || {
        move |request: super::super::DecideRequest, _: &dyn Fn() -> bool| {
            let name = request.checkpoint.to_string_lossy().into_owned();
            let _observer = started.send(name.clone());
            if name == "first" {
                let _release = release.recv_timeout(WAIT);
            }
            Ok(DecideResponse {
                values: serde_json::Map::new(),
                fields: Vec::new(),
            })
        }
    });
    let root = std::env::temp_dir().join(format!("mirmir-decide-timeout-{}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let application =
        crate::adapters::native::application(&AppConfig::default(), Store::new(paths));
    let (_shutdown, receiver) = watch::channel(false);
    let mut state = ApiState::new(application, None, receiver);
    state.decisions = Decisions { worker };
    let router = Router::new().route("/v1/decide", post(decide)).with_state(state).layer(
        TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_millis(150)),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/v1/decide", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    let body = |name: &str| {
        json!({
            "checkpoint": name, "state": {"text": "test"},
            "schema": {"type": "object", "properties": {"field": {"type": "boolean"}}}
        })
    };
    let client = reqwest::Client::new();
    let first = tokio::spawn(client.post(&url).json(&body("first")).send());
    assert_eq!(starts.recv_timeout(WAIT)?, "first");
    let abandoned = client.post(&url).json(&body("abandoned")).send().await?;
    assert_eq!(abandoned.status(), StatusCode::REQUEST_TIMEOUT);
    assert_eq!(first.await??.status(), StatusCode::REQUEST_TIMEOUT);
    gate.send(())?;
    let live = client.post(&url).json(&body("live")).send().await?;
    assert_eq!(live.status(), StatusCode::OK);
    assert_eq!(starts.recv_timeout(WAIT)?, "live");
    assert!(starts.try_recv().is_err(), "the timed-out job must never start");
    server.abort();
    Ok(())
}
