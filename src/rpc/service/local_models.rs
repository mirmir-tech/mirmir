use tonic::Status;

use super::RuntimeService;
use crate::{application::LocalModelInfo, rpc::proto};

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
        Self {
            id: value.id,
            repo_id: value.repo_id,
            revision: value.revision,
            commit: value.commit,
            path: value.path,
            state: value.state.as_str().to_owned(),
            recent_rank: value.recent_rank,
            selector: value.selector,
            managed: value.managed,
            image_input: value.image_input,
            image_unavailable_reason: value.image_unavailable_reason,
            model_class: value.model_class,
            loadable: value.loadable,
            load_unavailable_reason: value.load_unavailable_reason,
            library: value.library,
            ecosystem: value.ecosystem,
            container: value.container,
            encoding: value.encoding,
            metal_compatibility: value.metal_compatibility,
            cuda_compatibility: value.cuda_compatibility,
            size_bytes: value.size_bytes,
            tool_use: value.tool_use,
            thinking: value.thinking,
            vision: value.vision,
        }
    }
}

#[cfg(test)]
mod tests;
