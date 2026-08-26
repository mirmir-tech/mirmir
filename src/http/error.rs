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

    pub fn from_application(error: crate::application::Error) -> Self {
        use crate::application::Error;
        match error {
            Error::InvalidModel(message) => Self::bad_request(message),
            Error::ModelAlreadyLoading(message)
            | Error::ModelInUse(message)
            | Error::ModelLoaded(message) => {
                Self::new(StatusCode::CONFLICT, message, "invalid_request_error", "conflict")
            },
            Error::MemoryPressure(message)
            | Error::Inference(libmir::Error::MemoryAdmission { model: message, .. }) => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                message,
                "rate_limit_error",
                "rate_limit_exceeded",
            ),
            Error::Inference(
                error @ (libmir::Error::EmptyPrompt
                | libmir::Error::TaskMismatch { .. }
                | libmir::Error::Model(_)
                | libmir::Error::Context { .. }),
            ) => Self::bad_request(error.to_string()),
            Error::Inference(error @ libmir::Error::VisionResourceLimit { .. }) => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                error.to_string(),
                "rate_limit_error",
                "rate_limit_exceeded",
            ),
            error => Self::internal(error.to_string()),
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

    pub fn application_envelope(error: crate::application::Error) -> ErrorEnvelope {
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
