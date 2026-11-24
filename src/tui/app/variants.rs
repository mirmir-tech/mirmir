//! The quantization chosen for each GGUF repository in search results:
//! its default until ←/→ picks another, and the `repo:VARIANT` it downloads.

use super::App;
use crate::rpc::proto;

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
    pub(crate) fn pull_target(&self, model: &proto::CatalogModel) -> String {
        self.chosen_variant(model)
            .map_or_else(|| model.id.clone(), |variant| format!("{}:{}", model.id, variant.name))
    }

    /// Whether the row's chosen download is on this server: as the search
    /// found it, or in the local list since.
    pub(crate) fn catalog_downloaded(&self, model: &proto::CatalogModel) -> bool {
        let target = self.pull_target(model);
        let listed = self.local_models.iter().any(|local| {
            local.repo_id == target && matches!(local.state.as_str(), "available" | "ready")
        });
        listed
            || self.chosen_variant(model).map_or_else(
                || model.downloaded || matches!(model.local_source.as_str(), "mirmir" | "hf_cache"),
                |variant| variant.downloaded,
            )
    }

    /// The memory fit of the row's chosen download.
    pub(crate) fn catalog_fit<'a>(&self, model: &'a proto::CatalogModel) -> &'a str {
        self.chosen_variant(model)
            .map_or(&model.memory_fit, |variant| &variant.memory_fit)
    }
}
