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
        // On disk, as the server counts a download, loadable or not.
        let listed = self.local_models.iter().any(|local| {
            target.as_deref().is_some_and(|target| same_model(&local.repo_id, target))
                && matches!(local.state.as_str(), "available" | "ready" | "loading" | "error")
        });
        listed
            || self.chosen_variant(model).map_or_else(
                || model.downloaded || matches!(model.local_source.as_str(), "mirmir" | "hf_cache"),
                |variant| variant.downloaded,
            )
    }

    /// What loading or removing the row acts on: its chosen download when
    /// it is on this server, else the whole repository another tool's cache
    /// holds, which holds a repository whole whichever variant is chosen;
    /// `None` for a row to download first.
    pub(crate) fn catalog_local_target(&self, model: &proto::CatalogModel) -> Option<String> {
        if self.catalog_downloaded(model) {
            return self.pull_target(model).ok();
        }
        (model.local_source == "hf_cache").then(|| model.id.clone())
    }

    /// Whether the row stays listed under the compatible filter: a download
    /// it offers fits, whichever variant is chosen, so choosing another
    /// never hides the row being looked at.
    pub(crate) fn catalog_row_fits(model: &proto::CatalogModel) -> bool {
        let fits = |fit: &str| MemoryFit::from_protocol(fit) != Some(MemoryFit::DoesNotFit);
        if model.variants.is_empty() {
            fits(&model.memory_fit)
        } else {
            model.variants.iter().any(|variant| fits(&variant.memory_fit))
        }
    }

    /// Marks what `repo_id`, a model or a whole repository, names in the
    /// search results downloaded, or no longer.
    pub(super) fn mark_downloaded(&mut self, repo_id: &str, downloaded: bool) {
        let Ok(changed) = repo_id.parse::<HubReference>() else {
            return;
        };
        for model in &mut self.catalog {
            let Ok(reference) = model.id.parse::<HubReference>() else {
                continue;
            };
            // A variant is nothing of a row that lists none.
            if !reference.same_repository(&changed)
                || (changed.variant().is_some() && model.variants.is_empty())
            {
                continue;
            }
            for variant in &mut model.variants {
                let named = match changed.variant() {
                    Some(name) => variant.name.eq_ignore_ascii_case(name),
                    // A repository pulled whole takes its default variant;
                    // removed, it takes every variant along.
                    None => !downloaded || model.default_variant.as_ref() == Some(&variant.name),
                };
                if named {
                    variant.downloaded = downloaded;
                }
            }
            // The row's own download, as the search tells it: its default
            // variant, else the repository, which a variant's change leaves.
            let whole = changed.variant().is_none();
            model.downloaded = match &model.default_variant {
                Some(name) => {
                    model.variants.iter().any(|variant| &variant.name == name && variant.downloaded)
                },
                None if whole => downloaded,
                None => model.downloaded,
            };
            // A partial download or another tool's cache stays, but for the
            // repository's own removal, which takes them.
            if model.downloaded {
                "mirmir".clone_into(&mut model.local_source);
            } else if whole || model.local_source == "mirmir" {
                "remote".clone_into(&mut model.local_source);
            }
        }
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

/// Whether the references `left` and `right` name one model, in any
/// spelling; other selectors only when equal.
pub(super) fn same_model(left: &str, right: &str) -> bool {
    match (left.parse::<HubReference>(), right.parse::<HubReference>()) {
        (Ok(left), Ok(right)) => left.is_same_model(&right),
        _ => left == right,
    }
}

/// Whether the removal of `removed`, a model or a whole repository, takes
/// the model `other`.
pub(super) fn removes(removed: &str, other: &str) -> bool {
    match (removed.parse::<HubReference>(), other.parse::<HubReference>()) {
        (Ok(removed), Ok(other)) if removed.variant().is_none() => removed.same_repository(&other),
        _ => same_model(removed, other),
    }
}

/// A variant's size as the server spelled its kind.
pub fn variant_size(variant: &proto::CatalogVariant) -> VariantSize {
    VariantSize::from_protocol(&variant.size_kind, variant.weight_bytes)
}
