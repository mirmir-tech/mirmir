use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};

use crate::error::Result;

pub(super) async fn assert_activity_event(client: &Client, base: &str, cookie: &str) -> Result<()> {
    let response = client
        .get(format!("{base}/api/mirmir/v1/activity"))
        .header(header::COOKIE, cookie)
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let mut activity = response.bytes_stream();
    let event = tokio::time::timeout(Duration::from_secs(2), activity.next())
        .await
        .expect("activity event should arrive")
        .expect("activity stream should remain open")?;
    let event = String::from_utf8_lossy(&event);
    assert!(event.contains("event: activity"));
    assert!(event.contains("\"kind\":\"unload\""));
    assert!(event.contains("\"state\":\"completed\""));
    Ok(())
}
