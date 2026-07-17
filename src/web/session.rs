use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    http::{HeaderMap, StatusCode, header, uri::Authority},
    response::{IntoResponse, Response},
};
use serde::Serialize;

const COOKIE_NAME: &str = "mirmir_session";
const SESSION_LIMIT: usize = 64;
pub const TTL_SECONDS: u64 = 8 * 60 * 60;

#[derive(Clone, Default)]
pub struct Sessions(Arc<Mutex<HashMap<String, Session>>>);

struct Session {
    csrf: String,
    expires_at: u64,
}

pub struct Issued {
    pub token: String,
    pub csrf: String,
    pub expires_at: u64,
}

pub struct Authenticated {
    token: String,
}

#[derive(Debug)]
pub struct WebError {
    status: StatusCode,
    message: String,
    code: &'static str,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    message: String,
    code: &'static str,
}

impl Sessions {
    pub fn issue(&self) -> Result<Issued, WebError> {
        let token = random_token()?;
        let csrf = random_token()?;
        let expires_at = unix_seconds().saturating_add(TTL_SECONDS);
        let mut sessions = self.lock()?;
        sessions.retain(|_, session| session.expires_at > unix_seconds());
        if sessions.len() >= SESSION_LIMIT {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, session)| session.expires_at)
                .map(|(token, _)| token.clone());
            if let Some(oldest) = oldest {
                sessions.remove(&oldest);
            }
        }
        sessions.insert(token.clone(), Session { csrf: csrf.clone(), expires_at });
        drop(sessions);
        Ok(Issued { token, csrf, expires_at })
    }

    pub fn authenticate(&self, headers: &HeaderMap) -> Result<Authenticated, WebError> {
        let token = cookie(headers).ok_or_else(WebError::unauthorized)?;
        let mut sessions = self.lock()?;
        sessions.retain(|_, session| session.expires_at > unix_seconds());
        sessions
            .contains_key(&token)
            .then_some(Authenticated { token })
            .ok_or_else(WebError::unauthorized)
    }

    pub fn authorize_mutation(&self, headers: &HeaderMap) -> Result<Authenticated, WebError> {
        validate_origin(headers)?;
        let authenticated = self.authenticate(headers)?;
        let supplied = headers
            .get("x-mirmir-csrf")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(WebError::forbidden)?;
        let sessions = self.lock()?;
        let expected = sessions
            .get(&authenticated.token)
            .ok_or_else(WebError::unauthorized)?
            .csrf
            .clone();
        drop(sessions);
        if constant_time_eq(supplied.as_bytes(), expected.as_bytes()) {
            Ok(authenticated)
        } else {
            Err(WebError::forbidden())
        }
    }

    pub fn remove(&self, authenticated: &Authenticated) -> Result<(), WebError> {
        self.lock()?.remove(&authenticated.token);
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HashMap<String, Session>>, WebError> {
        self.0.lock().map_err(|_| WebError::internal("web session lock is poisoned"))
    }
}

pub fn validate_origin(headers: &HeaderMap) -> Result<(), WebError> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| WebError::forbidden_with("missing Host header"))?;
    let authority = host
        .parse::<Authority>()
        .map_err(|_| WebError::forbidden_with("invalid Host header"))?;
    if !local_host(authority.host()) {
        return Err(WebError::forbidden_with("web sessions require a loopback Host"));
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| WebError::forbidden_with("missing Origin header"))?;
    if origin == format!("http://{authority}") || origin == format!("https://{authority}") {
        Ok(())
    } else {
        Err(WebError::forbidden_with("Origin does not match Host"))
    }
}

fn local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|address| address.is_loopback())
}

fn cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .find_map(|value| value.strip_prefix(&format!("{COOKIE_NAME}=")).map(str::to_owned))
}

fn random_token() -> Result<String, WebError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| WebError::internal(format!("secure randomness unavailable: {error}")))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").map_err(|error| WebError::internal(error.to_string()))?;
    }
    Ok(token)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

impl WebError {
    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "missing or expired local web session", "web_session")
    }

    fn forbidden() -> Self {
        Self::forbidden_with("invalid or missing CSRF token")
    }

    fn forbidden_with(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message, "web_origin_or_csrf")
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message, "runtime_error")
    }

    pub fn from_status(status: tonic::Status) -> Self {
        let (http, code) = match status.code() {
            tonic::Code::InvalidArgument => (StatusCode::BAD_REQUEST, "invalid_request"),
            tonic::Code::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            tonic::Code::FailedPrecondition | tonic::Code::AlreadyExists => {
                (StatusCode::CONFLICT, "conflict")
            },
            tonic::Code::ResourceExhausted => (StatusCode::TOO_MANY_REQUESTS, "resource_limit"),
            tonic::Code::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "runtime_error"),
        };
        let message = status.message().to_owned();
        drop(status);
        Self::new(http, message, code)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message, "web_internal")
    }

    fn new(status: StatusCode, message: impl Into<String>, code: &'static str) -> Self {
        Self { status, message: message.into(), code }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            error: ErrorBody { message: self.message, code: self.code },
        };
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().extend(super::security_headers());
        response
    }
}
