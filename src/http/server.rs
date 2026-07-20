use std::{net::SocketAddr, time::Duration};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, StatusCode, header},
    routing::{get, post},
};
use tokio::{net::TcpListener, sync::watch, task::JoinHandle, time::timeout};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::{
    cors::CorsLayer,
    timeout::TimeoutLayer,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use super::{ApiState, handlers};
use crate::{
    config::ServerSettings,
    error::{Error, Result},
    rpc::RuntimeService,
};

pub struct Owner {
    address: SocketAddr,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<std::io::Result<()>>,
}

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn start(
    service: RuntimeService,
    settings: &ServerSettings,
    api_key: Option<String>,
) -> Result<Owner> {
    settings.validate()?;
    let listener = TcpListener::bind(settings.address()?).await?;
    let address = listener.local_addr()?;
    let (shutdown, mut receiver) = watch::channel(false);
    let router = router(service, settings, api_key, receiver.clone())?;
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                drop(receiver.changed().await);
            })
            .await
    });
    Ok(Owner { address, shutdown, task })
}

fn router(
    service: RuntimeService,
    settings: &ServerSettings,
    api_key: Option<String>,
    shutdown: watch::Receiver<bool>,
) -> Result<Router> {
    let router = Router::new().route("/health", get(handlers::health)).nest("/v1", v1_routes());
    let router = if settings.web_enabled {
        router
            .route("/", get(crate::web::redirect))
            .route("/ui", get(crate::web::redirect))
            .route("/ui/", get(crate::web::index))
            .route("/ui/app.css", get(crate::web::stylesheet))
            .route("/ui/app.js", get(crate::web::script))
            .route("/ui/sw.js", get(crate::web::service_worker))
            .route("/ui/assets/{*path}", get(crate::web::asset))
            .route("/api/mirmir/v1/bootstrap", get(crate::web::bootstrap))
            .route("/api/mirmir/v1/session", post(crate::web::create_session))
            .route("/api/mirmir/v1/session", axum::routing::delete(crate::web::delete_session))
            .route("/api/mirmir/v1/ws", get(crate::web::updates))
            .route("/api/mirmir/v1/overview", get(crate::web::overview))
            .route("/api/mirmir/v1/models", get(crate::web::models))
            .route("/api/mirmir/v1/configuration", get(crate::web::configuration))
            .route("/api/mirmir/v1/configuration", post(crate::web::update_configuration))
            .route("/api/mirmir/v1/activity", get(crate::web::activity))
            .route("/api/mirmir/v1/catalog/search", get(crate::web::search))
            .route("/api/mirmir/v1/models/inspect", get(crate::web::inspect))
            .route("/api/mirmir/v1/models/load", post(crate::web::load))
            .route("/api/mirmir/v1/models/unload", post(crate::web::unload))
            .route("/api/mirmir/v1/models/pull", post(crate::web::pull))
            .route("/api/mirmir/v1/models/remove", post(crate::web::remove))
            .route("/api/mirmir/v1/activity/cancel", post(crate::web::cancel))
            .route("/api/mirmir/v1/chat", post(crate::web::chat))
    } else {
        router
    };
    Ok(router
        .fallback(handlers::not_found)
        .with_state(ApiState::new(service, api_key, shutdown))
        .layer(DefaultBodyLimit::max(settings.body_limit_bytes))
        .layer(ConcurrencyLimitLayer::new(settings.max_concurrency))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            settings.request_timeout(),
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(cors(settings)?))
}

fn v1_routes() -> Router<ApiState> {
    Router::new()
        .route("/models", get(handlers::models))
        .route("/embeddings", post(crate::http::task::handlers::embeddings))
        .route("/rerank", post(crate::http::task::handlers::rerank))
        .route("/chat/completions", post(handlers::chat))
}

fn cors(settings: &ServerSettings) -> Result<CorsLayer> {
    let origins = settings
        .cors_origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .map_err(|error| Error::Config(format!("invalid CORS origin `{origin}`: {error}")))
        })
        .collect::<Result<Vec<_>>>()?;
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    Ok(if origins.is_empty() {
        cors
    } else {
        cors.allow_origin(origins)
    })
}

impl Owner {
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _shutdown_result = self.shutdown.send(true);
        if let Ok(result) = timeout(SHUTDOWN_TIMEOUT, &mut self.task).await {
            result??;
        } else {
            tracing::warn!("HTTP graceful shutdown timed out; closing active connections");
            self.task.abort();
            match self.task.await {
                Ok(result) => result?,
                Err(error) if error.is_cancelled() => {},
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
