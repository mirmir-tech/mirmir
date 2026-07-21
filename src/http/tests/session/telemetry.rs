use reqwest::{Client, StatusCode, header};
use serde_json::Value;

use crate::error::Result;

pub(super) async fn assert_telemetry_history(
    client: &Client,
    base: &str,
    cookie: &str,
) -> Result<()> {
    let response = client
        .get(format!("{base}/api/mirmir/v1/telemetry/history?limit=60"))
        .header(header::COOKIE, cookie)
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let history = response.json::<Value>().await?;
    assert!(history["samples"].is_array());
    assert_eq!(history["sampling_interval_ms"], 1_000);
    Ok(())
}
