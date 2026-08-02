use tonic::Status;

pub(super) fn load(error: &libmir::Error) -> Status {
    if matches!(error, libmir::Error::MemoryAdmission { .. }) {
        Status::resource_exhausted(error.to_string())
    } else {
        Status::internal(error.to_string())
    }
}
