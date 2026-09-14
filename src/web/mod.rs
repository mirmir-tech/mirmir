mod activity;
mod api;
mod capabilities;
mod chat;
mod configuration;
mod management;
mod operations;
mod session;
mod socket;
mod types;

pub use activity::activity;
pub use api::{create_session, delete_session, models, overview, telemetry_history};
use axum::{
    Json,
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
pub use chat::chat;
pub use configuration::{configuration, update_configuration};
pub use management::{cancel, inspect, load, remove, unload};
pub use operations::{pull, search};
use serde::Serialize;
pub use session::Sessions;
pub use socket::updates;

use crate::application::PROTOCOL_VERSION;

const INDEX: &str = include_str!(concat!(env!("OUT_DIR"), "/dashboard/index.html"));
const STYLESHEET: &str = include_str!("assets/app.css");
const DASHBOARD_MODULE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/dashboard/mirmir-dashboard.js"));
const DASHBOARD_WASM: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/dashboard/mirmir-dashboard_bg.wasm"));
const LOCKUP: &[u8] = include_bytes!("assets/brand/lockup.svg");
const FAVICON: &[u8] = include_bytes!("assets/brand/favicon.svg");
const TOPOGRAPHY: &[u8] = include_bytes!("assets/brand/topography.svg");
const SPACE_LATIN: &[u8] = include_bytes!("assets/fonts/space-grotesk-latin.woff2");
const SPACE_EXT: &[u8] = include_bytes!("assets/fonts/space-grotesk-latin-ext.woff2");
const INTER_LATIN: &[u8] = include_bytes!("assets/fonts/inter-latin.woff2");
const INTER_EXT: &[u8] = include_bytes!("assets/fonts/inter-latin-ext.woff2");
const MONO_LATIN: &[u8] = include_bytes!("assets/fonts/jetbrains-mono-latin.woff2");
const MONO_EXT: &[u8] = include_bytes!("assets/fonts/jetbrains-mono-latin-ext.woff2");

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum BackendSupport {
    Metal,
    Cuda,
    None,
}

impl BackendSupport {
    const fn for_build() -> Self {
        if cfg!(target_os = "macos") {
            Self::Metal
        } else if cfg!(target_os = "linux") {
            Self::Cuda
        } else {
            Self::None
        }
    }
}

#[derive(Serialize)]
struct Bootstrap {
    schema_version: u32,
    application: &'static str,
    server_version: &'static str,
    protocol_version: &'static str,
    ui_base: &'static str,
    management_api_base: &'static str,
    openai_api_base: &'static str,
    authentication: &'static str,
    platform: &'static str,
    architecture: &'static str,
    backend_support: BackendSupport,
    capabilities: Capabilities,
}

#[derive(Serialize)]
struct Capabilities {
    asset_delivery: &'static str,
    views: [&'static str; 4],
    management: &'static str,
    updates: &'static str,
}

pub async fn redirect() -> Redirect {
    Redirect::temporary("/ui/")
}

pub async fn index() -> Response {
    (asset_headers("text/html; charset=utf-8"), INDEX).into_response()
}

pub async fn stylesheet() -> Response {
    (asset_headers("text/css; charset=utf-8"), STYLESHEET).into_response()
}

pub async fn dashboard_module() -> Response {
    (asset_headers("text/javascript; charset=utf-8"), DASHBOARD_MODULE).into_response()
}

pub async fn dashboard_wasm() -> Response {
    (asset_headers("application/wasm"), DASHBOARD_WASM).into_response()
}

pub async fn asset(Path(path): Path<String>) -> Response {
    let Some((content_type, bytes)) = embedded_asset(&path) else {
        return (StatusCode::NOT_FOUND, security_headers(), "asset not found").into_response();
    };
    (asset_headers(content_type), bytes).into_response()
}

pub async fn bootstrap() -> Response {
    let bootstrap = Bootstrap {
        schema_version: 7,
        application: "mirmir",
        server_version: env!("CARGO_PKG_VERSION"),
        protocol_version: PROTOCOL_VERSION,
        ui_base: "/ui/",
        management_api_base: "/api/mirmir/v1",
        openai_api_base: "/v1",
        authentication: "local-session",
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        backend_support: BackendSupport::for_build(),
        capabilities: Capabilities {
            asset_delivery: "embedded",
            views: ["overview", "models", "chat", "configuration"],
            management: "model-lifecycle",
            updates: "websocket",
        },
    };
    (security_headers(), Json(bootstrap)).into_response()
}

fn asset_headers(content_type: &'static str) -> HeaderMap {
    let mut headers = security_headers();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert("x-mirmir-dashboard", HeaderValue::from_static("1"));
    headers
}

fn embedded_asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    let asset: (&str, &[u8]) = match path {
        "icons.svg" => ("image/svg+xml", include_bytes!("assets/icons.svg")),
        "theme.css" => ("text/css; charset=utf-8", include_bytes!("assets/theme.css")),
        "theme.js" => ("text/javascript; charset=utf-8", include_bytes!("assets/theme.js")),
        "brand/lockup-on-light.svg" => {
            ("image/svg+xml", include_bytes!("assets/brand/lockup-on-light.svg"))
        },
        "brand/lockup.svg" => ("image/svg+xml", LOCKUP),
        "brand/favicon.svg" => ("image/svg+xml", FAVICON),
        "brand/topography.svg" => ("image/svg+xml", TOPOGRAPHY),
        "fonts/space-grotesk-latin.woff2" => ("font/woff2", SPACE_LATIN),
        "fonts/space-grotesk-latin-ext.woff2" => ("font/woff2", SPACE_EXT),
        "fonts/inter-latin.woff2" => ("font/woff2", INTER_LATIN),
        "fonts/inter-latin-ext.woff2" => ("font/woff2", INTER_EXT),
        "fonts/jetbrains-mono-latin.woff2" => ("font/woff2", MONO_LATIN),
        "fonts/jetbrains-mono-latin-ext.woff2" => ("font/woff2", MONO_EXT),
        _ => return None,
    };
    Some(asset)
}

fn security_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; object-src 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    headers
}
