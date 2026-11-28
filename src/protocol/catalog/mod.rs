//! Single domain-to-wire projection for catalog models and variants shared by
//! gRPC (`rpc::proto`) and the web dashboard (`web::types`).

mod variant;

pub use variant::{WireCatalogVariant, WireModelVariants};

use crate::{
    catalog::{CatalogModel, SearchResults},
    model::format::{container_name, legacy_library},
    rpc::proto,
    web::types as web,
};

pub struct WireSearchResults {
    pub models: Vec<WireCatalogModel>,
    pub total_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub memory_source: String,
    pub next_cursor: Option<String>,
}

pub struct WireCatalogModel {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub gated: bool,
    pub model_class: String,
    pub compatibility: String,
    pub memory_fit: String,
    pub weight_bytes: Option<u64>,
    pub required_bytes: Option<u64>,
    pub budget_bytes: Option<u64>,
    pub confidence: String,
    pub reason: String,
    pub downloaded: bool,
    pub local_source: String,
    pub library: String,
    pub ecosystem: String,
    pub container: String,
    pub encoding: Option<String>,
    pub metal_compatibility: String,
    pub cuda_compatibility: String,
    pub preflight_bytes: Option<u64>,
    pub preflight_error: Option<String>,
    pub features: web::Features,
    pub variants: Vec<WireCatalogVariant>,
    pub default_variant: Option<String>,
}

impl From<SearchResults> for WireSearchResults {
    fn from(results: SearchResults) -> Self {
        Self {
            models: results.models.into_iter().map(WireCatalogModel::from).collect(),
            total_memory_bytes: results.memory.total,
            available_memory_bytes: results.memory.available,
            memory_source: results.memory.source.to_owned(),
            next_cursor: results.next_cursor,
        }
    }
}

impl From<CatalogModel> for WireCatalogModel {
    fn from(model: CatalogModel) -> Self {
        Self {
            id: model.id,
            downloads: model.downloads,
            likes: model.likes,
            gated: model.gated,
            model_class: model.model_class,
            compatibility: model.compatibility.as_str().to_owned(),
            memory_fit: model.memory_fit.as_str().to_owned(),
            weight_bytes: model.weight_bytes,
            required_bytes: model.required_bytes,
            budget_bytes: model.budget_bytes,
            confidence: model.confidence.as_str().to_owned(),
            reason: model.reason,
            downloaded: model.downloaded,
            local_source: model.local_source.as_str().to_owned(),
            library: legacy_library(model.container, &model.ecosystem).to_owned(),
            ecosystem: model.ecosystem.name().to_owned(),
            container: container_name(model.container).to_owned(),
            encoding: model.encoding.known_label(),
            metal_compatibility: model.metal_compatibility.as_str().to_owned(),
            cuda_compatibility: model.cuda_compatibility.as_str().to_owned(),
            preflight_bytes: model.preflight_bytes,
            preflight_error: model.preflight_error,
            features: web::Features {
                tool_use: model.features.tool_use,
                thinking: model.features.thinking,
                vision: model.features.vision,
            },
            variants: model.variants.into_iter().map(WireCatalogVariant::from).collect(),
            default_variant: model.default_variant,
        }
    }
}

impl WireSearchResults {
    pub fn into_proto(self) -> proto::SearchModelsResponse {
        proto::SearchModelsResponse {
            models: self.models.into_iter().map(WireCatalogModel::into_proto).collect(),
            total_memory_bytes: self.total_memory_bytes,
            available_memory_bytes: self.available_memory_bytes,
            memory_source: self.memory_source,
            next_cursor: self.next_cursor,
        }
    }

    pub fn into_web(self) -> web::CatalogResults {
        web::CatalogResults {
            models: self.models.into_iter().map(WireCatalogModel::into_web).collect(),
            total_memory_bytes: self.total_memory_bytes,
            available_memory_bytes: self.available_memory_bytes,
            memory_source: self.memory_source,
            next_cursor: self.next_cursor,
        }
    }
}

impl WireCatalogModel {
    pub fn into_proto(self) -> proto::CatalogModel {
        proto::CatalogModel {
            id: self.id,
            downloads: self.downloads,
            likes: self.likes,
            gated: self.gated,
            model_class: self.model_class,
            compatibility: self.compatibility,
            memory_fit: self.memory_fit,
            estimated_weight_bytes: self.weight_bytes,
            estimated_required_bytes: self.required_bytes,
            budget_bytes: self.budget_bytes,
            confidence: self.confidence,
            reason: self.reason,
            downloaded: self.downloaded,
            local_source: self.local_source,
            library: self.library,
            ecosystem: self.ecosystem,
            container: self.container,
            encoding: self.encoding,
            metal_compatibility: self.metal_compatibility,
            cuda_compatibility: self.cuda_compatibility,
            preflight_bytes: self.preflight_bytes,
            preflight_error: self.preflight_error,
            tool_use: self.features.tool_use,
            thinking: self.features.thinking,
            vision: self.features.vision,
            variants: self.variants.into_iter().map(WireCatalogVariant::into_proto).collect(),
            default_variant: self.default_variant,
        }
    }

    pub fn into_web(self) -> web::CatalogModel {
        web::CatalogModel {
            id: self.id,
            downloads: self.downloads,
            likes: self.likes,
            gated: self.gated,
            model_class: self.model_class,
            compatibility: self.compatibility,
            memory_fit: self.memory_fit,
            estimated_required_bytes: self.required_bytes,
            budget_bytes: self.budget_bytes,
            confidence: self.confidence,
            reason: self.reason,
            downloaded: self.downloaded,
            local_source: self.local_source,
            estimated_weight_bytes: self.weight_bytes,
            library: self.library,
            ecosystem: self.ecosystem,
            container: self.container,
            encoding: self.encoding,
            metal_compatibility: self.metal_compatibility,
            cuda_compatibility: self.cuda_compatibility,
            preflight_bytes: self.preflight_bytes,
            preflight_error: self.preflight_error,
            features: self.features,
            variants: self.variants.into_iter().map(WireCatalogVariant::into_web).collect(),
            default_variant: self.default_variant,
        }
    }
}
