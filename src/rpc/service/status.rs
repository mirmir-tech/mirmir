use std::fmt::Display;

use tonic::Status;

pub(super) fn internal<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::internal)
}

pub(super) fn invalid<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::invalid_argument)
}

pub(super) fn unavailable<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::unavailable)
}

fn convert<T, E: Display>(
    result: Result<T, E>,
    status: impl FnOnce(String) -> Status,
) -> Result<T, Status> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => Err(status(error.to_string())),
    }
}
