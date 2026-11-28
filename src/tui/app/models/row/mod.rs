//! Typed TUI catalog rows parsed once at gRPC ingress so rendering,
//! filtering, and actions never compare or mutate protocol strings.

mod local;

use libmir::AdmissionStatus;
pub use local::{ActivityRow, ActivityRowState, LocalRow, LocalRowState};

use crate::{
    catalog::{self, LocalSource, MemoryFit, VariantSize},
    rpc::proto,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFeatures {
    pub tool_use: bool,
    pub thinking: bool,
    pub vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRow {
    pub id: String,
    pub downloads: u64,
    pub gated: bool,
    pub compatibility: Option<AdmissionStatus>,
    pub memory_fit: Option<MemoryFit>,
    pub estimated_required_bytes: Option<u64>,
    pub downloaded: bool,
    pub local_source: LocalSource,
    pub ecosystem: String,
    pub encoding: Option<String>,
    pub features: ModelFeatures,
    pub variants: Vec<CatalogVariantRow>,
    pub default_variant: Option<String>,
}

impl CatalogRow {
    /// The server's catalog order of a row; a status this client does not know
    /// ranks last.
    pub fn rank(&self) -> u8 {
        self.compatibility
            .zip(self.memory_fit)
            .map_or(u8::MAX, |(compatibility, memory_fit)| catalog::rank(compatibility, memory_fit))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogVariantRow {
    pub name: String,
    pub size: VariantSize,
    pub required_bytes: Option<u64>,
    pub memory_fit: Option<MemoryFit>,
    pub downloaded: bool,
}

impl From<proto::CatalogModel> for CatalogRow {
    fn from(model: proto::CatalogModel) -> Self {
        Self {
            id: model.id,
            downloads: model.downloads,
            gated: model.gated,
            compatibility: catalog::admission_status(&model.compatibility),
            memory_fit: MemoryFit::from_protocol(&model.memory_fit),
            estimated_required_bytes: model.estimated_required_bytes,
            downloaded: model.downloaded,
            local_source: LocalSource::from_protocol(&model.local_source)
                .unwrap_or(LocalSource::Remote),
            ecosystem: model.ecosystem,
            encoding: model.encoding,
            features: ModelFeatures {
                tool_use: model.tool_use,
                thinking: model.thinking,
                vision: model.vision,
            },
            variants: model.variants.into_iter().map(CatalogVariantRow::from).collect(),
            default_variant: model.default_variant,
        }
    }
}

impl From<proto::CatalogVariant> for CatalogVariantRow {
    fn from(variant: proto::CatalogVariant) -> Self {
        Self {
            name: variant.name,
            size: VariantSize::from_protocol(&variant.size_kind, variant.weight_bytes),
            required_bytes: variant.required_bytes,
            memory_fit: MemoryFit::from_protocol(&variant.memory_fit),
            downloaded: variant.downloaded,
        }
    }
}
