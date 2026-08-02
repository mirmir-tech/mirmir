use std::{
    fmt::Display,
    sync::{Mutex, MutexGuard},
};

use tonic::Status;

pub(super) fn internal<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::internal)
}

pub(super) fn invalid<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::invalid_argument)
}

pub(super) fn failed_precondition<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::failed_precondition)
}

pub(super) fn unavailable<T, E: Display>(result: Result<T, E>) -> Result<T, Status> {
    convert(result, Status::unavailable)
}

pub(super) fn lock<'a, T>(mutex: &'a Mutex<T>, target: &str) -> Result<MutexGuard<'a, T>, Status> {
    let Ok(guard) = mutex.lock() else {
        return Err(Status::internal(format!("{target} lock is poisoned")));
    };
    Ok(guard)
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
