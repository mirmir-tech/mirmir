use std::fmt::Display;

use tonic::Status;

use crate::application::{Error, ErrorClass};

pub(super) fn application(error: Error) -> Status {
    let status = application_ref(&error);
    drop(error);
    status
}

pub(super) fn application_ref(error: &Error) -> Status {
    let message = error.to_string();
    match error.class() {
        ErrorClass::InvalidArgument => Status::invalid_argument(message),
        ErrorClass::InvalidModelOutput | ErrorClass::Conflict => {
            Status::failed_precondition(message)
        },
        ErrorClass::ResourceExhausted => Status::resource_exhausted(message),
        ErrorClass::Cancelled => Status::cancelled(message),
        ErrorClass::Unavailable => Status::unavailable(message),
        ErrorClass::Internal => Status::internal(message),
    }
}

pub(super) fn internal<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::internal)
}

pub(super) fn invalid<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::invalid_argument)
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
