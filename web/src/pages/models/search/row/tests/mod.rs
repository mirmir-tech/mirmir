use super::{pull_ended_without_install, variant_reason};
use crate::types::catalog::{CatalogModel, CatalogVariant};

/// Only a failed, cancelled or unreported pull releases a kept variant; a
/// completed one installed it.
#[test]
fn only_a_pull_that_installed_nothing_releases() {
    for state in [None, Some("failed"), Some("cancelled")] {
        assert!(pull_ended_without_install(state), "{state:?}");
    }
    for state in ["completed", "queued", "running", "cancelling"] {
        assert!(!pull_ended_without_install(Some(state)), "{state}");
    }
}

/// A variant's explanation follows the resolved variant's fit and size kind
/// rather than repeating a stale default reason from search.
#[test]
fn variant_reason_matches_resolved_fit_and_size_kind() {
    let model = CatalogModel {
        id: "demo/model".to_owned(),
        memory_fit: "fits".to_owned(),
        reason: "estimated weights and runtime reserve fit the current budget".to_owned(),
        metal_compatibility: "supported".to_owned(),
        cuda_compatibility: "supported".to_owned(),
        ..Default::default()
    };
    assert_eq!(variant_reason(&model, None), model.reason);

    let q8_exceeds = CatalogVariant {
        name: "Q8_0".to_owned(),
        size_kind: "exact".to_owned(),
        weight_bytes: Some(9 << 30),
        memory_fit: "does_not_fit".to_owned(),
    };
    assert_eq!(
        variant_reason(&model, Some(&q8_exceeds)),
        "weights and runtime reserve exceed the current memory budget"
    );

    let q4_fits = CatalogVariant {
        name: "Q4_K_M".to_owned(),
        size_kind: "exact".to_owned(),
        weight_bytes: Some(5 << 30),
        memory_fit: "fits".to_owned(),
    };
    assert_eq!(
        variant_reason(&model, Some(&q4_fits)),
        "weights and runtime reserve fit the current budget"
    );

    let unsupported = CatalogModel {
        metal_compatibility: "unsupported".to_owned(),
        cuda_compatibility: "unsupported".to_owned(),
        reason: "model contract or weight format is not supported by libmir".to_owned(),
        ..model
    };
    assert_eq!(variant_reason(&unsupported, Some(&q4_fits)), unsupported.reason);
}

#[cfg(target_arch = "wasm32")]
mod browser;
