use std::sync::Arc;

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use super::*;

#[derive(Clone)]
struct MockFile {
    bytes: Arc<Vec<u8>>,
    mode: ResponseMode,
}

#[derive(Clone, Copy)]
enum ResponseMode {
    Range,
    IgnoreRange,
    WrongContentRange,
}

#[tokio::test]
async fn fetches_only_safetensors_prefix_and_header()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let header =
        r#"{"model.embed_tokens.weight":{"dtype":"BF16","shape":[2,2],"data_offsets":[0,8]}}"#;
    let bytes = safetensors(header, 1024 * 1024)?;
    let (base, server) = server(bytes, ResponseMode::Range).await?;

    let result = inspect_files_at(
        &reqwest::Client::new(),
        None,
        &base,
        "Org/Model",
        "revision",
        &["model.safetensors".into()],
        remote_metadata(),
    )
    .await?;
    server.abort();

    assert_eq!(result.catalog.len(), 1);
    assert_eq!(result.catalog.tensors[0].dtype, "BF16");
    assert_eq!(result.fetched_bytes, u64::try_from(8 + header.len())?);
    assert_eq!(result.files, vec!["model.safetensors"]);
    assert_eq!(result.revision, "revision");
    Ok(())
}

#[tokio::test]
async fn rejects_a_server_that_ignores_the_range_header()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let bytes = safetensors("{}", 1024 * 1024)?;
    let (base, server) = server(bytes, ResponseMode::IgnoreRange).await?;

    let error = inspect_files_at(
        &reqwest::Client::new(),
        None,
        &base,
        "Org/Model",
        "main",
        &["model.safetensors".into()],
        remote_metadata(),
    )
    .await
    .err()
    .ok_or_else(|| std::io::Error::other("full payload response was accepted"))?;
    server.abort();

    assert!(error.to_string().contains("ignored byte range"));
    Ok(())
}

#[tokio::test]
async fn rejects_a_mismatched_content_range() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let bytes = safetensors("{}", 1024)?;
    let (base, server) = server(bytes, ResponseMode::WrongContentRange).await?;

    let url = file_url(&base, "Org/Model", "main", "model.safetensors")?;
    let error = range::fetch(&reqwest::Client::new(), None, url, 0, 7)
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("wrong Content-Range was accepted"))?;
    server.abort();

    assert!(error.to_string().contains("Content-Range"));
    Ok(())
}

#[test]
fn index_selects_only_referenced_shards() -> Result<()> {
    let available = vec![
        "model-00001-of-00002.safetensors".to_owned(),
        "model-00002-of-00002.safetensors".to_owned(),
        "adapter.safetensors".to_owned(),
    ];
    let bytes = br#"{"weight_map":{"a":"model-00002-of-00002.safetensors","b":"model-00001-of-00002.safetensors"}}"#;

    let files = index::shard_files(bytes, &available)?;

    assert_eq!(
        files,
        vec!["model-00001-of-00002.safetensors", "model-00002-of-00002.safetensors"]
    );
    Ok(())
}

#[test]
fn index_rejects_a_missing_shard() {
    let bytes = br#"{"weight_map":{"a":"missing.safetensors"}}"#;

    let error = index::shard_files(bytes, &["model.safetensors".to_owned()])
        .expect_err("missing shard should fail preflight");

    assert!(error.to_string().contains("missing shard"));
}

#[test]
fn parses_config_and_tokenizer_metadata() -> Result<()> {
    let parsed = metadata::parse(
        br#"{"model_type":"mistral"}"#,
        Some(br#"{"chat_template":"{{ messages }}"}"#),
    )?;

    assert_eq!(parsed.config["model_type"], "mistral");
    assert_eq!(
        parsed
            .tokenizer_config
            .as_ref()
            .and_then(|value| value["chat_template"].as_str()),
        Some("{{ messages }}")
    );
    Ok(())
}

fn remote_metadata() -> RemoteModelMetadata {
    RemoteModelMetadata {
        config: serde_json::json!({"model_type": "test"}),
        tokenizer_config: None,
        modules: None,
        pooling: None,
        sentence_transformers: None,
        processor_config: None,
        tokenizer_assets: tokenizer_assets(),
    }
}

pub(super) fn tokenizer_assets() -> libmir::TokenizerAssets {
    libmir::TokenizerAssets {
        kind: libmir::TokenizerKind::TokenizerJson,
        primary: "tokenizer.json".into(),
        merges: None,
        metadata: Vec::new(),
        total_bytes: 10,
    }
}

fn safetensors(
    header: &str,
    payload_bytes: usize,
) -> std::result::Result<Vec<u8>, std::num::TryFromIntError> {
    let mut bytes = u64::try_from(header.len())?.to_le_bytes().to_vec();
    bytes.extend_from_slice(header.as_bytes());
    bytes.resize(bytes.len().saturating_add(payload_bytes), 0);
    Ok(bytes)
}

async fn server(
    bytes: Vec<u8>,
    mode: ResponseMode,
) -> std::io::Result<(reqwest::Url, tokio::task::JoinHandle<()>)> {
    let app = Router::new()
        .route("/{*path}", get(range_response))
        .with_state(MockFile { bytes: Arc::new(bytes), mode });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let task = tokio::spawn(async move {
        drop(axum::serve(listener, app).await);
    });
    let url = match reqwest::Url::parse(&format!("http://{address}/")) {
        Ok(url) => url,
        Err(error) => return Err(std::io::Error::other(error)),
    };
    Ok((url, task))
}

async fn range_response(State(file): State<MockFile>, headers: HeaderMap) -> Response {
    if matches!(file.mode, ResponseMode::IgnoreRange) {
        return (StatusCode::OK, Bytes::copy_from_slice(&file.bytes)).into_response();
    }
    let Some((start, end)) = headers.get(header::RANGE).and_then(parse_range) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(bytes) = file.bytes.get(start..=end) else {
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    };
    let content_range = if matches!(file.mode, ResponseMode::WrongContentRange) {
        format!("bytes 1-{end}/{}", file.bytes.len())
    } else {
        format!("bytes {start}-{end}/{}", file.bytes.len())
    };
    (
        StatusCode::PARTIAL_CONTENT,
        [(header::CONTENT_RANGE, content_range)],
        Bytes::copy_from_slice(bytes),
    )
        .into_response()
}

fn parse_range(value: &reqwest::header::HeaderValue) -> Option<(usize, usize)> {
    let value = value.to_str().ok()?.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?))
}
