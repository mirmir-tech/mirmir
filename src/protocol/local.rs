//! Single domain-to-wire projection for local models shared by gRPC
//! (`rpc::proto`) and the web dashboard (`web::types`).

use crate::{
    application::LocalModelInfo,
    model::format::{container_name, legacy_library},
    rpc::proto,
    web::types as web,
};

pub struct WireLocalModel {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub path: String,
    pub state: String,
    pub recent_rank: Option<u32>,
    pub selector: String,
    pub managed: bool,
    pub image_input: bool,
    pub image_unavailable_reason: String,
    pub model_class: String,
    pub loadable: bool,
    pub load_unavailable_reason: String,
    pub library: String,
    pub ecosystem: String,
    pub container: String,
    pub encoding: Option<String>,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
    pub size_bytes: u64,
    pub features: web::Features,
}

impl From<LocalModelInfo> for WireLocalModel {
    fn from(model: LocalModelInfo) -> Self {
        Self {
            id: model.id,
            repo_id: model.repo_id,
            revision: model.revision,
            commit: model.commit,
            path: model.path,
            state: model.state.as_str().to_owned(),
            recent_rank: model.recent_rank,
            selector: model.selector,
            managed: model.managed,
            image_input: model.image_input,
            image_unavailable_reason: model.image_unavailable_reason,
            model_class: model.model_class,
            loadable: model.loadable,
            load_unavailable_reason: model.load_unavailable_reason,
            library: legacy_library(model.container, &model.ecosystem).to_owned(),
            ecosystem: model.ecosystem.name().to_owned(),
            container: container_name(model.container).to_owned(),
            encoding: model.encoding.known_label(),
            metal_compatibility: model.metal_compatibility.as_str().to_owned(),
            cuda_compatibility: model.cuda_compatibility.as_str().to_owned(),
            size_bytes: model.size_bytes,
            features: web::Features {
                tool_use: model.tool_use,
                thinking: model.thinking,
                vision: model.vision,
            },
        }
    }
}

impl WireLocalModel {
    pub fn into_proto(self) -> proto::LocalModelInfo {
        proto::LocalModelInfo {
            id: self.id,
            repo_id: self.repo_id,
            revision: self.revision,
            commit: self.commit,
            path: self.path,
            state: self.state,
            recent_rank: self.recent_rank,
            selector: self.selector,
            managed: self.managed,
            image_input: self.image_input,
            image_unavailable_reason: self.image_unavailable_reason,
            model_class: self.model_class,
            loadable: self.loadable,
            load_unavailable_reason: self.load_unavailable_reason,
            library: self.library,
            ecosystem: self.ecosystem,
            container: self.container,
            encoding: self.encoding,
            metal_compatibility: self.metal_compatibility,
            cuda_compatibility: self.cuda_compatibility,
            size_bytes: self.size_bytes,
            tool_use: self.features.tool_use,
            thinking: self.features.thinking,
            vision: self.features.vision,
        }
    }

    pub fn into_web(self) -> web::Model {
        web::Model {
            id: self.id,
            repo_id: self.repo_id,
            revision: self.revision,
            commit: self.commit,
            path: self.path,
            state: self.state,
            selector: self.selector,
            managed: self.managed,
            image_input: self.image_input,
            image_unavailable_reason: self.image_unavailable_reason,
            loadable: self.loadable,
            load_unavailable_reason: self.load_unavailable_reason,
            class: self.model_class,
            library: self.library,
            ecosystem: self.ecosystem,
            container: self.container,
            encoding: self.encoding,
            metal_compatibility: self.metal_compatibility,
            cuda_compatibility: self.cuda_compatibility,
            size_bytes: self.size_bytes,
            features: self.features,
        }
    }
}
