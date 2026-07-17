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
    assert_eq!(index.headers()["x-frame-options"], "DENY");
    assert_eq!(
        index.headers()["content-security-policy"],
        "default-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
    );
    let index = index.text().await?;
    assert!(index.contains("MiRMiR · Runtime dashboard"));
    assert!(index.contains("/ui/assets/brand/lockup.svg"));

    let stylesheet = client.get(format!("{base}/ui/app.css")).send().await?;
    assert_eq!(stylesheet.status(), StatusCode::OK);
    let stylesheet = stylesheet.text().await?;
    assert!(stylesheet.contains("#79d7ff"));
    assert!(stylesheet.contains("Space Grotesk"));

    let script = client.get(format!("{base}/ui/app.js")).send().await?;
    assert_eq!(script.status(), StatusCode::OK);
    assert!(script.text().await?.contains("/api/mirmir/v1"));

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
    assert_eq!(bootstrap["application"], "mirmir");
    assert_eq!(bootstrap["management_api_base"], "/api/mirmir/v1");
    assert_eq!(bootstrap["capabilities"]["management"], "model-lifecycle");
    assert_eq!(
        bootstrap["capabilities"]["views"],
        json!(["overview", "models", "chat", "configuration", "activity"])
    );
    assert!(bootstrap["protocol_version"].is_string());

    owner.shutdown().await
}
