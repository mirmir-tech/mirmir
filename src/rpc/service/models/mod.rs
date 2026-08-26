use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

mod capability;
mod error;
mod events;
mod stream;
#[cfg(test)]
mod tests;

pub use self::stream::stream_load;
pub(super) use self::{error::load as load_error, events::log_progress};

impl RuntimeService {
    pub(super) fn list(&self) -> Result<Vec<proto::ModelInfo>, Status> {
        self.coordinator
            .models()
            .map(|models| models.into_iter().map(proto::ModelInfo::from).collect())
            .map_err(|error| Status::internal(error.to_string()))
    }

    pub(super) fn unload(&self, selector: &str) -> Result<bool, Status> {
        self.coordinator.unload_model(selector).map_err(|error| load_error(&error))
    }
}
