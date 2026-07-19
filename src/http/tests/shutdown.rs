use std::time::Duration;

use reqwest::{Client, StatusCode, header};
use serde_json::Value;

use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    http::start,
    rpc::RuntimeService,
};

#[tokio::test]
async fn open_activity_stream_does_not_block_shutdown() -> Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-shutdown-{}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let mut config = AppConfig::default();
    config.server.http_bind = "127.0.0.1:0".to_owned();
    config.server.web_enabled = true;
    let service = RuntimeService::new(&config, Store::new(paths));
    let owner = start(service, &config.server, None).await?;
    let base = format!("http://{}", owner.address());
    let client = Client::new();
    let session = client
        .post(format!("{base}/api/mirmir/v1/session"))
        .header(header::ORIGIN, &base)
        .send()
        .await?;
    assert_eq!(session.status(), StatusCode::OK);
    let cookie = session.headers()[header::SET_COOKIE]
        .to_str()
        .expect("session cookie should be ASCII")
        .split(';')
        .next()
        .expect("session cookie should have a value")
        .to_owned();
    let _csrf = session.json::<Value>().await?;
    let activity = client
        .get(format!("{base}/api/mirmir/v1/activity"))
        .header(header::COOKIE, cookie)
        .send()
        .await?;
    assert_eq!(activity.status(), StatusCode::OK);

    tokio::time::timeout(Duration::from_secs(1), owner.shutdown())
        .await
        .expect("open SSE connection must not block shutdown")?;
    drop(activity);
    Ok(())
}
