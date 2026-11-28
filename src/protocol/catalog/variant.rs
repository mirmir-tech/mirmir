//! Single domain-to-wire projection for catalog variants shared by gRPC
//! (`rpc::proto`) and the web dashboard (`web::types`).

use crate::{
    catalog::{CatalogVariant, ModelVariants},
    rpc::proto,
    web::types as web,
};

pub struct WireCatalogVariant {
    pub name: String,
    pub size_kind: String,
    pub weight_bytes: Option<u64>,
    pub required_bytes: Option<u64>,
    pub memory_fit: String,
    pub downloaded: bool,
}

pub struct WireModelVariants {
    pub variants: Vec<WireCatalogVariant>,
    pub default_variant: Option<String>,
}

impl From<CatalogVariant> for WireCatalogVariant {
    fn from(variant: CatalogVariant) -> Self {
        Self {
            name: variant.name,
            size_kind: variant.size.kind().to_owned(),
            weight_bytes: variant.size.bytes(),
            required_bytes: variant.required_bytes,
            memory_fit: variant.memory_fit.as_str().to_owned(),
            downloaded: variant.downloaded,
        }
    }
}

impl From<ModelVariants> for WireModelVariants {
    fn from(listed: ModelVariants) -> Self {
        Self {
            variants: listed.variants.into_iter().map(WireCatalogVariant::from).collect(),
            default_variant: listed.default_variant,
        }
    }
}

impl WireCatalogVariant {
    pub fn into_proto(self) -> proto::CatalogVariant {
        proto::CatalogVariant {
            name: self.name,
            size_kind: self.size_kind,
            weight_bytes: self.weight_bytes,
            required_bytes: self.required_bytes,
            memory_fit: self.memory_fit,
            downloaded: self.downloaded,
        }
    }

    pub fn into_web(self) -> web::CatalogVariant {
        web::CatalogVariant {
            name: self.name,
            size_kind: self.size_kind,
            weight_bytes: self.weight_bytes,
            required_bytes: self.required_bytes,
            memory_fit: self.memory_fit,
            downloaded: self.downloaded,
        }
    }
}

impl WireModelVariants {
    pub fn into_proto(self) -> proto::ListModelVariantsResponse {
        proto::ListModelVariantsResponse {
            variants: self.variants.into_iter().map(WireCatalogVariant::into_proto).collect(),
            default_variant: self.default_variant,
        }
    }

    pub fn into_web(self) -> web::ModelVariants {
        web::ModelVariants {
            variants: self.variants.into_iter().map(WireCatalogVariant::into_web).collect(),
            default_variant: self.default_variant,
        }
    }
}
