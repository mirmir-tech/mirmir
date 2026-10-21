//! HTTP contract of `POST /v1/decide`. Only the last test needs a Laya
//! checkpoint; every rejection is decided before a model would load.

use std::time::Duration;

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};

use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    http::{server::Owner, start},
};

const KEY: &str = "decide-key";

async fn server(name: &str) -> Result<(Owner, String)> {
    let root = std::env::temp_dir().join(format!("mirmir-decide-{name}-{}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let mut config = AppConfig::default();
    config.server.http_bind = "127.0.0.1:0".to_owned();
    let application = crate::adapters::native::application(&config, Store::new(paths));
    let owner = start(application, &config.server, Some(KEY.to_owned())).await?;
    let base = format!("http://{}/v1/decide", owner.address());
    Ok((owner, base))
}

fn request(checkpoint: &str, property: &Value) -> Value {
    json!({
        "checkpoint": checkpoint,
        "state": {"text": "We were billed twice for March."},
        "schema": {"type": "object", "properties": {"field": property}}
    })
}

async fn rejected(client: &Client, base: &str, body: Value) -> Result<Value> {
    let response = client.post(base).bearer_auth(KEY).json(&body).send().await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    let error = response.json::<Value>().await?;
    assert_eq!(error["error"]["type"], "invalid_request_error", "{body}");
    Ok(error)
}

#[tokio::test]
async fn rejects_invalid_requests_and_keeps_the_worker_answering() -> Result<()> {
    let (owner, base) = server("contract").await?;
    let client = Client::new();
    let valid = request("/nonexistent/laya", &json!({"type": "boolean"}));

    let anonymous = client.post(&base).json(&valid).send().await?;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let wrong = client.post(&base).bearer_auth("other").json(&valid).send().await?;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    for body in ["{", "[]", r#"{"checkpoint": "x"}"#] {
        let response = client
            .post(&base)
            .bearer_auth(KEY)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await?;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
    }
    let mut unsupported_backend = valid.clone();
    unsupported_backend["backend"] = json!("tpu");
    rejected(&client, &base, unsupported_backend).await?;

    for property in [
        json!({"type": "integer", "minimum": i64::MIN, "maximum": i64::MAX}),
        json!({"type": "integer", "minimum": 0, "maximum": 4, "multipleOf": 2}),
        json!({"type": "string"}),
        json!({"const": true, "enum": [true, false]}),
    ] {
        rejected(&client, &base, request("/nonexistent/laya", &property)).await?;
    }

    // The worker survived every rejection: it still reaches the model load.
    let missing = rejected(&client, &base, valid).await?;
    assert!(
        missing["error"]["message"]
            .as_str()
            .is_some_and(|text| !text.contains("worker"))
    );

    tokio::time::timeout(Duration::from_secs(3), owner.shutdown())
        .await
        .map_err(|_| crate::error::Error::Config("shutdown timed out".into()))?
}

/// Needs a Laya checkpoint directory in `MIRMIR_LAYA_MODEL`.
#[tokio::test]
#[ignore = "needs a Laya checkpoint in MIRMIR_LAYA_MODEL"]
async fn answers_after_a_rejected_request() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let checkpoint = std::env::var("MIRMIR_LAYA_MODEL")?;
    let (owner, base) = server("checkpoint").await?;
    let client = Client::new();
    let extreme = json!({"type": "integer", "minimum": i64::MIN, "maximum": i64::MAX});
    rejected(&client, &base, request(&checkpoint, &extreme)).await?;

    let body = json!({
        "checkpoint": checkpoint,
        "state": {"text": "We were billed twice for March. Please refund the duplicate."},
        "schema": {"type": "object", "properties": {
            "refund": {"type": "boolean", "description": "Does the customer want a refund?"},
            "only": {"const": false},
            "urgency": {"type": "integer", "minimum": 1, "maximum": 3}
        }}
    });
    let response = client.post(&base).bearer_auth(KEY).json(&body).send().await?;
    assert_eq!(response.status(), StatusCode::OK);
    let answer = response.json::<Value>().await?;
    assert_eq!(answer["values"]["only"], json!(false));
    assert!(answer["values"]["refund"].is_boolean());
    assert!(
        answer["values"]["urgency"]
            .as_i64()
            .is_some_and(|level| (1..=3).contains(&level))
    );
    let fields = answer["fields"].as_array().ok_or("fields must be an array")?;
    assert_eq!(fields.len(), 3);
    for field in fields {
        for key in ["name", "options", "probabilities", "confidence", "concentration"] {
            assert!(!field[key].is_null(), "{key} in {field}");
        }
    }
    let urgency = fields.iter().find(|field| field["name"] == "urgency").ok_or("urgency")?;
    assert!(urgency["expected_level"].is_number());
    Ok(owner.shutdown().await?)
}
