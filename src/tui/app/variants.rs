//! The quantization chosen for each GGUF repository in search results:
//! its default until ←/→ picks another, and the `repo:VARIANT` it downloads.

use super::App;
use crate::{
    catalog::{MemoryFit, VariantSize},
    config::HubReference,
    error::Result,
    rpc::proto,
};

impl App {
    /// The variant a row downloads: the one picked, else its default.
    pub(crate) fn chosen_variant<'a>(
        &self,
        model: &'a proto::CatalogModel,
    ) -> Option<&'a proto::CatalogVariant> {
        let name = self.catalog_variants.get(&model.id).or(model.default_variant.as_ref())?;
        model.variants.iter().find(|variant| &variant.name == name)
    }

    /// Moves the selected row's variant `step` places, fewest bits first.
    pub(crate) fn cycle_variant(&mut self, step: isize) {
        let Some(model) = self.selected_catalog_model() else {
            return;
        };
        let count = model.variants.len();
        if count == 0 {
            return;
        }
        let current = self
            .chosen_variant(model)
            .and_then(|chosen| model.variants.iter().position(|variant| variant == chosen))
            .unwrap_or(0);
        let Some(next) = current.checked_add_signed(step).filter(|&next| next < count) else {
            return;
        };
        let (id, name) = (model.id.clone(), model.variants[next].name.clone());
        self.catalog_variants.insert(id, name);
    }

    /// What a row's download pulls: `repo:VARIANT` for a GGUF repository.
    pub(crate) fn pull_target(&self, model: &proto::CatalogModel) -> Result<String> {
        let reference = model.id.parse::<HubReference>()?;
        let target = match self.chosen_variant(model) {
            Some(variant) => reference.with_variant(&variant.name)?,
            None => reference,
        };
        Ok(target.to_string())
    }

    /// Whether the row's chosen download is on this server: as the search
    /// found it, or in the local list since.
    pub(crate) fn catalog_downloaded(&self, model: &proto::CatalogModel) -> bool {
        let target = self.pull_target(model).ok();
        let listed = self.local_models.iter().any(|local| {
            target.as_ref() == Some(&local.repo_id)
                && matches!(local.state.as_str(), "available" | "ready")
        });
        listed
            || self.chosen_variant(model).map_or_else(
                || model.downloaded || matches!(model.local_source.as_str(), "mirmir" | "hf_cache"),
                |variant| variant.downloaded,
            )
    }

    /// The memory fit of the row's chosen download; unknown for a fit this
    /// client does not know.
    pub(crate) fn catalog_fit(&self, model: &proto::CatalogModel) -> MemoryFit {
        let fit = self
            .chosen_variant(model)
            .map_or(&model.memory_fit, |variant| &variant.memory_fit);
        MemoryFit::from_protocol(fit).unwrap_or(MemoryFit::Unknown)
    }
}

/// A variant's size as the server spelled its kind.
pub fn variant_size(variant: &proto::CatalogVariant) -> VariantSize {
    VariantSize::from_protocol(&variant.size_kind, variant.weight_bytes)
}
