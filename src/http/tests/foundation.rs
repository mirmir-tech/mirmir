use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    http::start,
    rpc::RuntimeService,
};

static NEXT_WEB: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn serves_opt_in_embedded_web_foundation() -> Result<()> {
    let id = NEXT_WEB.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-web-{}-{id}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let mut config = AppConfig::default();
    config.server.http_bind = "127.0.0.1:0".to_owned();
    config.server.web_enabled = true;
    let service = RuntimeService::new(&config, Store::new(paths));
    let owner = start(service, &config.server, Some("openai-key".to_owned())).await?;
    let base = format!("http://{}", owner.address());
    let client = reqwest::Client::new();

    let index = client.get(format!("{base}/ui/")).send().await?;
    assert_eq!(index.status(), StatusCode::OK);
    assert_eq!(index.headers()["x-mirmir-dashboard"], "1");
    assert_eq!(index.headers()["x-frame-options"], "DENY");
    assert_eq!(
        index.headers()["content-security-policy"],
        "default-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
    );
    let index = index.text().await?;
    assert!(index.contains("MiRMiR · Runtime dashboard"));
    assert!(index.contains("/ui/assets/brand/lockup.svg"));
    assert!(index.contains("/ui/assets/brand/favicon.svg?v=2"));
    assert!(index.contains("/ui/app.css?v=5"));
    assert!(index.contains("/ui/app.js?v=7"));
    assert!(index.contains("Starting runtime"));
    assert!(index.contains("id=\"connection-state\""));
    assert!(index.contains("<th>Model class</th>"));
    assert!(index.contains("id=\"load-top-k\" type=\"number\" min=\"0\""));
    assert!(index.contains("id=\"load-task-capabilities\""));

    let stylesheet = client.get(format!("{base}/ui/app.css")).send().await?;
    assert_eq!(stylesheet.status(), StatusCode::OK);
    let stylesheet = stylesheet.text().await?;
    assert!(stylesheet.contains("#79d7ff"));
    assert!(stylesheet.contains("Space Grotesk"));
    assert!(!stylesheet.contains(".badge.loading::before"));
    assert!(stylesheet.contains(".dialog-actions button[aria-busy=\"true\"]::before"));
    assert!(stylesheet.contains(".badge.loading, .badge.queued, .badge.running"));
    assert!(stylesheet.contains(".badge.unavailable"));

    let script = client.get(format!("{base}/ui/app.js")).send().await?;
    assert_eq!(script.status(), StatusCode::OK);
    let script = script.text().await?;
    assert!(script.contains("/api/mirmir/v1"));
    assert!(script.contains("new WebSocket"));
    assert!(script.contains("Connection lost"));
    assert!(!script.contains("new EventSource"));
    assert!(!script.contains("setInterval"));
    assert!(script.contains("serviceWorker.register"));
    assert!(script.contains("pullOperations"));
    assert!(script.contains("appendPullRow"));
    assert!(script.contains("addEventListener(\"invalid\""));
    assert!(script.contains("inspection.task === \"generation\""));
    assert!(script.contains("!inspection.task && settings != null"));
    assert!(script.contains("capabilities.max_input_tokens"));
    assert!(script.contains("input.disabled = !enabled"));
    assert!(script.contains("setLoadButtonState(\"inspecting\")"));
    assert!(script.contains("button.setAttribute(\"aria-busy\", \"true\")"));
    assert!(script.contains("new Set([\"queued\", \"running\", \"cancelling\"])"));
    assert!(script.contains("partial download will be removed"));
    assert!(script.contains("model.load_unavailable_reason"));

    let worker = client.get(format!("{base}/ui/sw.js")).send().await?;
    assert_eq!(worker.status(), StatusCode::OK);
    assert_eq!(worker.headers()["content-type"], "text/javascript; charset=utf-8");
    let worker = worker.text().await?;
    assert!(worker.contains("mirmir-dashboard-shell"));
    assert!(worker.contains("x-mirmir-dashboard"));

    let logo = client.get(format!("{base}/ui/assets/brand/lockup.svg")).send().await?;
    assert_eq!(logo.status(), StatusCode::OK);
    assert_eq!(logo.headers()["content-type"], "image/svg+xml");
    assert!(logo.text().await?.contains("aria-label=\"MiRMiR\""));

    let font = client.get(format!("{base}/ui/assets/fonts/inter-latin.woff2")).send().await?;
    assert_eq!(font.status(), StatusCode::OK);
    assert_eq!(font.headers()["content-type"], "font/woff2");
    assert!(font.bytes().await?.len() > 10_000);

    let missing_asset = client.get(format!("{base}/ui/assets/unknown.svg")).send().await?;
    assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);

    let bootstrap = client.get(format!("{base}/api/mirmir/v1/bootstrap")).send().await?;
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let bootstrap = bootstrap.json::<Value>().await?;
    assert_eq!(bootstrap["schema_version"], 3);
    assert_eq!(bootstrap["application"], "mirmir");
    assert_eq!(bootstrap["management_api_base"], "/api/mirmir/v1");
    assert_eq!(bootstrap["capabilities"]["management"], "model-lifecycle");
    assert_eq!(bootstrap["capabilities"]["updates"], "websocket");
    assert_eq!(
        bootstrap["capabilities"]["views"],
        json!(["overview", "models", "chat", "configuration", "activity"])
    );
    assert!(bootstrap["protocol_version"].is_string());

    let inspect = client
        .get(format!("{base}/api/mirmir/v1/models/inspect?selector=probe"))
        .send()
        .await?;
    assert_eq!(inspect.status(), StatusCode::UNAUTHORIZED);

    owner.shutdown().await
}
