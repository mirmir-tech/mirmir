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

    /// Unloads off the async runtime: unloading waits for the memory gate,
    /// which a load may hold.
    pub(super) async fn unload_blocking(&self, selector: String) -> Result<bool, Status> {
        let service = self.clone();
        super::status::internal(
            tokio::task::spawn_blocking(move || service.unload(&selector)).await,
        )?
    }
}
