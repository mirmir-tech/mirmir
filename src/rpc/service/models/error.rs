use tonic::Status;

pub(in crate::rpc::service) fn load(error: &crate::application::Error) -> Status {
    match error {
        crate::application::Error::InvalidModel(_) => Status::invalid_argument(error.to_string()),
        crate::application::Error::ModelAlreadyLoading(_)
        | crate::application::Error::ModelInUse(_)
        | crate::application::Error::ModelLoaded(_) => {
            Status::failed_precondition(error.to_string())
        },
        crate::application::Error::MemoryPressure(_)
        | crate::application::Error::Inference(libmir::Error::MemoryAdmission { .. }) => {
            Status::resource_exhausted(error.to_string())
        },
        _ => Status::internal(error.to_string()),
    }
}
