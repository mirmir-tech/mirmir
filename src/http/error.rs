use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

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
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message, "server_error", "runtime_error")
    }

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

    pub fn from_application(error: &crate::application::Error) -> Self {
        use crate::application::ErrorClass;
        let class = error.class();
        let message = error.to_string();
        match class {
            ErrorClass::InvalidArgument => Self::bad_request(message),
            ErrorClass::Conflict => {
                Self::new(StatusCode::CONFLICT, message, "invalid_request_error", "conflict")
            },
            ErrorClass::ResourceExhausted => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                message,
                "rate_limit_error",
                "rate_limit_exceeded",
            ),
            ErrorClass::Cancelled => {
                Self::new(StatusCode::REQUEST_TIMEOUT, message, "request_error", "cancelled")
            },
            ErrorClass::Unavailable => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                message,
                "server_error",
                "service_unavailable",
            ),
            ErrorClass::Internal => Self::internal(message),
        }
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

    pub fn application_envelope(error: &crate::application::Error) -> ErrorEnvelope {
        let error = Self::from_application(error);
        Self::envelope(error.message, error.kind, error.code)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_is_retryable_unavailable() {
        let error = crate::application::Error::RuntimeNotReady;
        assert_eq!(
            ApiError::from_application(&error).into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn impossible_cache_request_is_http_400_not_retryable_server_failure() {
        let error = libmir::Error::Runtime(libmir::RuntimeError::KvCapacity {
            requested: 4679,
            capacity: 4096,
        });
        let response = ApiError::from_application(&error.into()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn temporary_memory_admission_remains_http_429() {
        let error = libmir::Error::MemoryAdmission {
            model: "test".into(),
            required_bytes: 20,
            available_bytes: 10,
        };
        assert_eq!(
            ApiError::from_application(&error.into()).into_response().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
