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
    assert!(index.contains("/ui/assets/brand/favicon.svg?v=3"));
    assert_simplified_shell(&index);
    assert!(index.contains("id=\"toast-region\""));
    assert!(index.contains("id=\"connection-state\""));
    assert!(index.contains("<th>Features</th>"));
    assert!(index.contains("aria-label=\"Close model search\""));
    assert!(!index.contains("id=\"catalog-query\""));
    assert!(index.contains("id=\"load-top-k-range\" type=\"range\""));
    assert!(index.contains("id=\"load-task-capabilities\""));
    assert!(index.contains("class=\"composer-dock\""));
    assert!(index.contains("aria-label=\"Add attachment\""));
    assert!(index.contains("M12 5v14M5 12h14"));
    assert!(index.contains("aria-label=\"Generation performance\""));
    assert!(index.contains("id=\"throughput-chart\""));
    assert!(index.contains("id=\"memory-chart\""));
    assert!(index.contains("id=\"kv-chart\""));
    assert!(index.contains("id=\"activity-list\""));
    assert!(!index.contains("data-view=\"activity\""));

    let stylesheet = client.get(format!("{base}/ui/app.css")).send().await?;
    assert_eq!(stylesheet.status(), StatusCode::OK);
    let stylesheet = stylesheet.text().await?;
    assert_stylesheet(&stylesheet);

    let script = client.get(format!("{base}/ui/app.js")).send().await?;
    assert_eq!(script.status(), StatusCode::OK);
    let script = script.text().await?;
    assert_script(&script);

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
    assert_eq!(bootstrap["schema_version"], 6);
    assert_eq!(bootstrap["application"], "mirmir");
    assert_eq!(bootstrap["management_api_base"], "/api/mirmir/v1");
    assert_eq!(bootstrap["capabilities"]["management"], "model-lifecycle");
    assert_eq!(bootstrap["capabilities"]["updates"], "websocket");
    assert_eq!(
        bootstrap["capabilities"]["views"],
        json!(["overview", "models", "chat", "configuration"])
    );
    assert!(bootstrap["protocol_version"].is_string());

    let inspect = client
        .get(format!("{base}/api/mirmir/v1/models/inspect?selector=probe"))
        .send()
        .await?;
    assert_eq!(inspect.status(), StatusCode::UNAUTHORIZED);

    owner.shutdown().await
}

fn assert_script(script: &str) {
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
    assert!(script.contains("optimisticModelStates.clear()"));
    assert!(script.contains("rangeInput.addEventListener(\"input\""));
    assert!(script.contains("setLoadButtonState(\"inspecting\")"));
    assert!(script.contains("button.setAttribute(\"aria-busy\", \"true\")"));
    assert!(script.contains("const statePill"));
    assert!(script.contains("error ? 8000 : 5000"));
    assert!(script.contains("dialog.show()"));
    assert!(script.contains("event.key === \"Escape\""));
    assert!(script.contains("new AbortController()"));
    assert!(script.contains("[\"load\", \"restore\", \"unload\", \"pull\"]"));
    assert!(script.contains("const renderMarkdown"));
    assert!(script.contains("const updateReasoningState"));
    assert!(script.contains("const renderTimeChart"));
    assert!(script.contains("/telemetry/history?limit=900"));
    assert!(script.contains("dashboardWindowMinutes"));
    assert!(script.contains("const submittedImage = chatImage"));
    assert!(script.contains("chatImage = null"));
    assert!(script.contains("Response stopped at the ${data.completion_tokens}-token limit"));
    assert!(script.contains("log.scrollTop = log.scrollHeight"));
    assert!(!script.contains("scrollIntoView"));
}

fn assert_stylesheet(stylesheet: &str) {
    assert!(stylesheet.contains("#79d7ff"));
    assert!(stylesheet.contains("Space Grotesk"));
    assert!(!stylesheet.contains(".badge.loading::before"));
    assert!(stylesheet.contains(".dialog-actions button[aria-busy=\"true\"]::before"));
    assert!(stylesheet.contains(".badge.loading, .badge.queued, .badge.running"));
    assert!(stylesheet.contains(".badge.unavailable"));
    assert!(stylesheet.contains("overscroll-behavior: contain"));
    assert!(stylesheet.contains(".dashboard-grid"));
    assert!(stylesheet.contains(".telemetry-chart"));
    assert!(stylesheet.contains(".activity-timeline"));
}

fn assert_simplified_shell(index: &str) {
    assert!(index.contains("/ui/app.css?v=22"));
    assert!(index.contains("/ui/app.js?v=28"));
    assert!(!index.contains("class=\"page-heading"));
    assert!(!index.contains("MIRMIR RUNTIME / WEB"));
    assert!(!index.contains("<footer>"));
    assert!(!index.contains("startup-banner"));
    assert!(!index.contains("id=\"more-models\""));
    assert!(!index.contains("id=\"model-count\""));
}
