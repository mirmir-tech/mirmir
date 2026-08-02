use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tonic::Code;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
    kind: &'static str,
    code: &'static str,
}

#[derive(Serialize)]
pub struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    message: String,
    #[serde(rename = "type")]
    kind: &'static str,
    param: Option<String>,
    code: &'static str,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message, "invalid_request_error", "invalid_request")
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid or missing bearer token",
            "authentication_error",
            "invalid_api_key",
        )
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message, "invalid_request_error", "not_found")
    }

    pub fn from_status(status: tonic::Status) -> Self {
        let (http, kind, code) = match status.code() {
            Code::InvalidArgument => {
                (StatusCode::BAD_REQUEST, "invalid_request_error", "invalid_request")
            },
            Code::NotFound => (StatusCode::NOT_FOUND, "invalid_request_error", "not_found"),
            Code::FailedPrecondition | Code::AlreadyExists => {
                (StatusCode::CONFLICT, "invalid_request_error", "conflict")
            },
            Code::ResourceExhausted => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limit_error", "rate_limit_exceeded")
            },
            Code::Unauthenticated => {
                (StatusCode::UNAUTHORIZED, "authentication_error", "invalid_api_key")
            },
            Code::PermissionDenied => {
                (StatusCode::FORBIDDEN, "permission_error", "permission_denied")
            },
            Code::Unavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "server_error", "service_unavailable")
            },
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "server_error", "runtime_error"),
        };
        let message = status.message().to_owned();
        drop(status);
        Self::new(http, message, kind, code)
    }

    fn new(
        status: StatusCode,
        message: impl Into<String>,
        kind: &'static str,
        code: &'static str,
    ) -> Self {
        Self {
            status,
            message: message.into(),
            kind,
            code,
        }
    }

    pub fn envelope(
        message: impl Into<String>,
        kind: &'static str,
        code: &'static str,
    ) -> ErrorEnvelope {
        ErrorEnvelope {
            error: ErrorBody {
                message: message.into(),
                kind,
                param: None,
                code,
            },
        }
    }

    pub fn status_envelope(status: tonic::Status) -> ErrorEnvelope {
        let error = Self::from_status(status);
        Self::envelope(error.message, error.kind, error.code)
    }
}

pub fn status<T>(result: Result<T, tonic::Status>) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(ApiError::from_status(error)),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let unauthorized = self.status == StatusCode::UNAUTHORIZED;
        let body = Self::envelope(self.message, self.kind, self.code);
        let mut response = (self.status, Json(body)).into_response();
        if unauthorized {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}
