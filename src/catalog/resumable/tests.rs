use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    Router,
    extract::{Path as Route, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};

use super::*;
use crate::config::Paths;

const FIRST: &str = "1111111111111111111111111111111111111111";
const SECOND: &str = "2222222222222222222222222222222222222222";
/// Two revisions of one file, the same length.
const BYTES: [&[u8]; 2] = [b"first revision bytes", b"second revision byte"];

/// A Hub serving the file at two commits, `main` naming the second once
/// `moved`; ranges are honoured, as the Hub does.
async fn file(
    State(moved): State<Arc<AtomicBool>>,
    Route((_, _, revision, _)): Route<(String, String, String, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let bytes = match revision.as_str() {
        FIRST => BYTES[0],
        SECOND => BYTES[1],
        "main" if moved.load(Ordering::SeqCst) => BYTES[1],
        "main" => BYTES[0],
        _ => return (StatusCode::NOT_FOUND, Vec::new()).into_response(),
    };
    let offset = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|range| range.strip_prefix("bytes=")?.strip_suffix('-')?.parse::<usize>().ok());
    let (status, bytes) = offset.map_or((StatusCode::OK, bytes), |offset| {
        (StatusCode::PARTIAL_CONTENT, &bytes[offset..])
    });
    (status, bytes.to_vec()).into_response()
}

/// A transfer interrupted at `main`'s first commit resumes after `main` has
/// moved to a same-length second revision: the blob is the pinned
/// revision's, byte for byte, and `refs/main` names it.
#[tokio::test]
async fn a_resumed_file_keeps_its_pinned_revision() -> Result<()> {
    let moved = Arc::new(AtomicBool::new(false));
    let router = Router::new()
        .route("/{owner}/{name}/resolve/{revision}/{*file}", get(file))
        .with_state(moved.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, router).await });

    let root = std::env::temp_dir().join(format!("mirmir-pinned-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let store =
        Store::new(Paths::from_roots(root.join("config"), root.join("state"), &root.join("run")));
    let pinned = pin(&store, "owner/repo", FIRST).await?;
    let pinned = Pinned { requested: "main".to_owned(), ..pinned };
    let blobs = store.paths().hub_cache_dir.join("models--owner--repo/blobs");
    std::fs::create_dir_all(&blobs)?;
    std::fs::write(blobs.join("blob.incomplete"), &BYTES[0][..6])?;
    moved.store(true, Ordering::SeqCst);

    let files = vec![ListedFile {
        path: "weights/model.bin".to_owned(),
        etag: "blob".to_owned(),
        size: BYTES[0].len() as u64,
    }];
    let (updates, _received) = mpsc::channel(64);
    let snapshot = transfer(
        &store,
        &endpoint,
        ("owner/repo", &pinned),
        files,
        &updates,
        &CancellationToken::new(),
    )
    .await?;
    server.abort();
    assert_eq!(std::fs::read(blobs.join("blob"))?, BYTES[0]);
    assert_eq!(std::fs::read(snapshot.join("weights/model.bin"))?, BYTES[0]);
    assert!(snapshot.ends_with(FIRST));
    let reference = store.paths().hub_cache_dir.join("models--owner--repo/refs/main");
    assert_eq!(std::fs::read_to_string(reference)?, format!("{FIRST}\n"));
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn file_urls_name_the_commit_under_any_endpoint() -> Result<()> {
    for endpoint in ["https://hub.example", "https://hub.example/", "http://127.0.0.1:8080/mirror/"]
    {
        let url = download_url(endpoint, "owner/repo", SECOND, "dir/model file.gguf")?;
        let prefix = endpoint.trim_end_matches('/');
        assert_eq!(url, format!("{prefix}/owner/repo/resolve/{SECOND}/dir/model%20file.gguf"));
    }
    Ok(())
}
