use tonic::Status;

use super::RuntimeService;
use crate::{application::LocalModelInfo, protocol::local::WireLocalModel, rpc::proto};

impl RuntimeService {
    pub(super) fn local_models(&self) -> Result<Vec<proto::LocalModelInfo>, Status> {
        self.application
            .local_models()
            .map(|models| models.into_iter().map(Into::into).collect())
            .map_err(super::status::application)
    }
}

impl From<LocalModelInfo> for proto::LocalModelInfo {
    fn from(value: LocalModelInfo) -> Self {
        WireLocalModel::from(value).into_proto()
    }
}

#[cfg(test)]
mod tests;
