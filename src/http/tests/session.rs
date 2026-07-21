use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    http::start,
    rpc::RuntimeService,
};

mod activity;
mod telemetry;

use activity::assert_activity_event;
use telemetry::assert_telemetry_history;

static NEXT_SESSION: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn isolates_web_sessions_and_requires_csrf_for_mutations() -> Result<()> {
    let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-session-{}-{id}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let mut config = AppConfig::default();
    config.server.http_bind = "127.0.0.1:0".to_owned();
    config.server.web_enabled = true;
    let service = RuntimeService::new(&config, Store::new(paths));
    let owner = start(service, &config.server, Some("openai-key".to_owned())).await?;
    let base = format!("http://{}", owner.address());
    let client = Client::new();
    let (cookie, csrf) = create_session(&client, &base).await?;

    let overview = client
        .get(format!("{base}/api/mirmir/v1/overview"))
        .header(header::COOKIE, &cookie)
        .send()
        .await?;
    assert_eq!(overview.status(), StatusCode::OK);
    let overview = overview.json::<Value>().await?;
    assert_eq!(overview["loaded_models"], 0);
    assert!(overview.get("last_prefill_tokens_per_second").is_some());
    assert!(overview.get("last_decode_tokens_per_second").is_some());

    assert_telemetry_history(&client, &base, &cookie).await?;

    let models = client
        .get(format!("{base}/api/mirmir/v1/models"))
        .header(header::COOKIE, &cookie)
        .send()
        .await?;
    assert_eq!(models.status(), StatusCode::OK);
    assert!(models.json::<Value>().await?["models"].is_array());
    assert_configuration(&client, &base, &cookie, &csrf).await?;
    assert_invalid_chat(&client, &base, &cookie, &csrf).await?;

    let unload = client
        .post(format!("{base}/api/mirmir/v1/models/unload"))
        .header(header::ORIGIN, &base)
        .header(header::COOKIE, &cookie)
        .header("x-mirmir-csrf", &csrf)
        .json(&json!({ "selector": "not-loaded" }))
        .send()
        .await?;
    assert_eq!(unload.status(), StatusCode::OK);
    assert_eq!(unload.json::<Value>().await?["unloaded"], false);

    let invalid_pull = client
        .post(format!("{base}/api/mirmir/v1/models/pull"))
        .header(header::ORIGIN, &base)
        .header(header::COOKIE, &cookie)
        .header("x-mirmir-csrf", &csrf)
        .json(&json!({ "repo_id": "" }))
        .send()
        .await?;
    assert_eq!(invalid_pull.status(), StatusCode::BAD_REQUEST);

    assert_activity_event(&client, &base, &cookie).await?;

    let missing_csrf = client
        .delete(format!("{base}/api/mirmir/v1/session"))
        .header(header::ORIGIN, &base)
        .header(header::COOKIE, &cookie)
        .send()
        .await?;
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

    let logout = client
        .delete(format!("{base}/api/mirmir/v1/session"))
        .header(header::ORIGIN, &base)
        .header(header::COOKIE, &cookie)
        .header("x-mirmir-csrf", csrf)
        .send()
        .await?;
    assert_eq!(logout.status(), StatusCode::OK);

    let expired = client
        .get(format!("{base}/api/mirmir/v1/overview"))
        .header(header::COOKIE, &cookie)
        .send()
        .await?;
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);

    owner.shutdown().await
}

async fn create_session(client: &Client, base: &str) -> Result<(String, String)> {
    let unauthorized = client
        .get(format!("{base}/api/mirmir/v1/overview"))
        .bearer_auth("openai-key")
        .send()
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let missing_origin = client.post(format!("{base}/api/mirmir/v1/session")).send().await?;
    assert_eq!(missing_origin.status(), StatusCode::FORBIDDEN);
    let foreign_origin = client
        .post(format!("{base}/api/mirmir/v1/session"))
        .header(header::ORIGIN, "https://example.com")
        .send()
        .await?;
    assert_eq!(foreign_origin.status(), StatusCode::FORBIDDEN);

    let session = client
        .post(format!("{base}/api/mirmir/v1/session"))
        .header(header::ORIGIN, base)
        .send()
        .await?;
    assert_eq!(session.status(), StatusCode::OK);
    let set_cookie = session.headers()[header::SET_COOKIE]
        .to_str()
        .expect("session cookie should be ASCII")
        .to_owned();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    assert!(set_cookie.contains("Path=/api/mirmir/v1"));
    let cookie = set_cookie
        .split(';')
        .next()
        .expect("session cookie should have a value")
        .to_owned();
    let csrf = session.json::<Value>().await?["csrf_token"]
        .as_str()
        .expect("session should contain a CSRF token")
        .to_owned();
    Ok((cookie, csrf))
}

async fn assert_configuration(client: &Client, base: &str, cookie: &str, csrf: &str) -> Result<()> {
    let configuration = client
        .get(format!("{base}/api/mirmir/v1/configuration"))
        .header(header::COOKIE, cookie)
        .send()
        .await?;
    assert_eq!(configuration.status(), StatusCode::OK);
    assert!(configuration.json::<Value>().await?["values"].is_array());

    let value = update_configuration(
        client,
        base,
        cookie,
        csrf,
        json!({
            "operation": "set_value",
            "key": "server.request_timeout_seconds",
            "value": "301"
        }),
    )
    .await?;
    assert_eq!(value.status(), StatusCode::OK);
    assert_eq!(value.json::<Value>().await?["restart_required"], true);

    let secret = update_configuration(
        client,
        base,
        cookie,
        csrf,
        json!({ "operation": "set_value", "key": "hugging_face.token", "value": "hf_test_secret" }),
    )
    .await?;
    assert_eq!(secret.status(), StatusCode::OK);
    let secret = secret.json::<Value>().await?;
    let row = secret["configuration"]["values"]
        .as_array()
        .and_then(|values| values.iter().find(|value| value["key"] == "hugging_face.token"))
        .expect("HF token should be an ordinary configuration row");
    assert_eq!(row["value"], "********");
    assert_eq!(row["kind"], "secret");
    assert!(row["actions"].as_array().is_some_and(|actions| {
        actions.iter().any(|action| action == "edit")
            && actions.iter().any(|action| action == "test")
            && actions.iter().any(|action| action == "remove")
    }));
    assert!(!secret.to_string().contains("hf_test_secret"));

    let removed = update_configuration(
        client,
        base,
        cookie,
        csrf,
        json!({ "operation": "remove_value", "key": "hugging_face.token" }),
    )
    .await?;
    assert_eq!(removed.status(), StatusCode::OK);
    Ok(())
}

async fn update_configuration(
    client: &Client,
    base: &str,
    cookie: &str,
    csrf: &str,
    body: Value,
) -> Result<reqwest::Response> {
    Ok(client
        .post(format!("{base}/api/mirmir/v1/configuration"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, cookie)
        .header("x-mirmir-csrf", csrf)
        .json(&body)
        .send()
        .await?)
}

async fn assert_invalid_chat(client: &Client, base: &str, cookie: &str, csrf: &str) -> Result<()> {
    let response = client
        .post(format!("{base}/api/mirmir/v1/chat"))
        .header(header::ORIGIN, base)
        .header(header::COOKIE, cookie)
        .header("x-mirmir-csrf", csrf)
        .json(&json!({ "model": "", "messages": [] }))
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
}
