use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::StatusCode;
use serde_json::{Value, json};

use super::start;
use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    rpc::RuntimeService,
};

mod foundation;
mod session;
mod settings;
mod shutdown;

static NEXT_HTTP: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn serves_openai_routes_with_bearer_auth_and_sse_errors() -> Result<()> {
    let id = NEXT_HTTP.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-http-{}-{id}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let mut config = AppConfig::default();
    config.server.http_bind = "127.0.0.1:0".to_owned();
    let service = RuntimeService::new(&config, Store::new(paths));
    let owner = start(service, &config.server, Some("test-key".to_owned())).await?;
    let base = format!("http://{}", owner.address());
    let client = reqwest::Client::new();

    let health = client.get(format!("{base}/health")).send().await?;
    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(health.json::<Value>().await?["status"], "ok");

    let disabled_web = client.get(format!("{base}/ui/")).send().await?;
    assert_eq!(disabled_web.status(), StatusCode::NOT_FOUND);

    let unauthorized = client.get(format!("{base}/v1/models")).send().await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unauthorized.json::<Value>().await?["error"]["code"], "invalid_api_key");

    let models = client.get(format!("{base}/v1/models")).bearer_auth("test-key").send().await?;
    assert_eq!(models.status(), StatusCode::OK);
    assert_eq!(models.json::<Value>().await?["data"], json!([]));

    let missing = client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth("test-key")
        .json(&json!({
            "model": "missing",
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await?;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    let missing = missing.json::<Value>().await?;
    assert!(missing["error"]["message"].is_string());
    assert_eq!(missing["error"]["type"], "invalid_request_error");

    let streamed = client
        .post(format!("{base}/v1/chat/completions"))
        .bearer_auth("test-key")
        .json(&json!({
            "model": "missing",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        }))
        .send()
        .await?;
    assert_eq!(streamed.status(), StatusCode::OK);
    let body = streamed.text().await?;
    assert!(body.contains("chat.completion.chunk"));
    assert!(body.contains("invalid_request_error"));
    assert!(body.contains("[DONE]"));

    owner.shutdown().await
}
