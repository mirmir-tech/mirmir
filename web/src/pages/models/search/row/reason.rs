//! Compatibility and per-variant memory explanations for a catalog row.

use super::super::unsupported;
use crate::types::catalog::{CatalogModel, CatalogVariant};

const DEFAULT_UNSUPPORTED_REASON: &str =
    "model contract or weight format is not supported by libmir";

/// Badge state shown in `StatePill` when the row is neither downloading nor
/// already downloaded.
pub(super) fn badge_state(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    if unsupported(model) {
        return "unsupported".to_owned();
    }
    variant.map_or_else(|| model.memory_fit.clone(), |variant| variant.memory_fit.clone())
}

/// Subtitle under the model name: preserves the server's incompatibility
/// reason when the model is unsupported, otherwise explains the resolved
/// variant's memory fit.
pub(super) fn subtitle(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    if unsupported(model) {
        return server_reason(model);
    }
    memory_reason(model, variant)
}

/// Tooltip on `StatePill`: preserves the server's rejection reason for
/// unsupported models and appends the resolved variant's memory explanation.
pub(super) fn tooltip(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    if !unsupported(model) {
        return memory_reason(model, variant);
    }
    let reason = server_reason(model);
    match variant.map(variant_memory_reason).filter(|text| !text.is_empty()) {
        Some(memory) => format!("{reason} · {memory}"),
        None => reason,
    }
}

/// Short memory-fit label shown separately beside a variant's size when the
/// row's main badge is occupied by `unsupported`.
pub(super) fn separate_fit_label(
    model: &CatalogModel,
    variant: Option<&CatalogVariant>,
) -> Option<String> {
    if !unsupported(model) {
        return None;
    }
    variant.map(|variant| variant.memory_fit.replace(['_', '-'], " "))
}

/// Memory explanation matching the resolved variant and its size precision,
/// or the repository's own reason when it has no variants.
pub(super) fn memory_reason(model: &CatalogModel, variant: Option<&CatalogVariant>) -> String {
    let Some(variant) = variant else {
        return model.reason.clone();
    };
    let text = variant_memory_reason(variant);
    if !text.is_empty() {
        text
    } else if variant.memory_fit == model.memory_fit {
        model.reason.clone()
    } else {
        String::new()
    }
}

fn server_reason(model: &CatalogModel) -> String {
    if model.reason.is_empty() {
        DEFAULT_UNSUPPORTED_REASON.to_owned()
    } else {
        model.reason.clone()
    }
}

fn variant_memory_reason(variant: &CatalogVariant) -> String {
    let exact = variant.size_kind == "exact";
    match (variant.memory_fit.as_str(), exact) {
        ("fits", true) => "weights and runtime reserve fit the current budget".to_owned(),
        ("fits", false) => {
            "estimated weights and runtime reserve fit the current budget".to_owned()
        },
        ("tight", true) => "weights and runtime reserve leave less than 15% headroom".to_owned(),
        ("tight", false) => "estimated usage leaves less than 15% headroom".to_owned(),
        ("does_not_fit", true) => {
            "weights and runtime reserve exceed the current memory budget".to_owned()
        },
        ("does_not_fit", false) => "estimated usage exceeds the current memory budget".to_owned(),
        ("unknown", _) => "insufficient Hub or device metadata for a reliable estimate".to_owned(),
        _ => String::new(),
    }
}
