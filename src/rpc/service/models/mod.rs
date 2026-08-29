use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

mod capability;
mod events;
mod stream;
#[cfg(test)]
mod tests;

pub(super) use self::events::log_progress;
pub use self::stream::stream_load;

impl RuntimeService {
    pub(super) fn list(&self) -> Result<Vec<proto::ModelInfo>, Status> {
        self.application
            .models()
            .map(|models| models.into_iter().map(proto::ModelInfo::from).collect())
            .map_err(super::status::application)
    }

    pub(super) fn unload(&self, selector: &str) -> Result<bool, Status> {
        self.application.unload_model(selector).map_err(super::status::application)
    }
}
