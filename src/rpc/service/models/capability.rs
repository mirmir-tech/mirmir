use crate::{application::ModelInfo, rpc::proto};

impl From<ModelInfo> for proto::ModelInfo {
    fn from(model: ModelInfo) -> Self {
        Self {
            id: model.id,
            path: model.path,
            image_input: model.image_input,
            image_unavailable_reason: model.image_unavailable_reason,
        }
    }
}
