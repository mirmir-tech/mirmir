use super::session::WebError;

pub(super) fn status<T>(result: Result<T, tonic::Status>) -> Result<T, WebError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(WebError::from_status(error)),
    }
}

pub(super) fn runtime<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<T, WebError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(WebError::runtime(error.to_string())),
    }
}
