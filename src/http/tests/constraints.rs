use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    http::start,
};

#[tokio::test]
async fn rejects_invalid_json_requests_before_streaming_or_model_work() -> Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-json-validation-{}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let mut config = AppConfig::default();
    config.server.http_bind = "127.0.0.1:0".to_owned();
    let application = crate::adapters::native::application(&config, Store::new(paths));
    application
        .restore_active_models()
        .map_err(|error| crate::error::Error::Config(error.to_string()))?;
    let owner = start(application.clone(), &config.server, Some("test-key".to_owned())).await?;
    let client = reqwest::Client::new();
    for stream in [false, true] {
        for (field, value, message) in [
            ("min_tokens", json!(5), "min_tokens=0"),
            ("ignore_eos", json!(true), "ignore_eos=false"),
            ("repetition_penalty", json!(1.1), "repetition_penalty=1"),
            (
                "response_format",
                json!({"type":"json_schema", "json_schema":{
                "name":"test", "schema":{"type":"nonsense"}}}),
                "tool constraints",
            ),
        ] {
            let mut body = json!({"model":"missing", "stream":stream,
                "messages":[{"role":"user","content":"hello"}],
                "response_format":{"type":"json_object"}});
            body[field] = value;
            let response = client
                .post(format!("http://{}/v1/chat/completions", owner.address()))
                .bearer_auth("test-key")
                .json(&body)
                .send()
                .await?;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(
                response.headers()["content-type"]
                    .to_str()
                    .unwrap_or_default()
                    .starts_with("application/json")
            );
            let body = response.json::<Value>().await?;
            assert!(
                body["error"]["message"].as_str().unwrap_or_default().contains(message),
                "{body}"
            );
        }
    }
    let telemetry = application
        .telemetry_snapshot()
        .map_err(|error| crate::error::Error::Config(error.to_string()))?;
    assert_eq!(telemetry.total_requests, 0);
    owner.shutdown().await?;
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}
