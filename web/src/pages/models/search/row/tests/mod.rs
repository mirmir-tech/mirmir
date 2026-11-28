use super::{
    pull_ended_without_install,
    reason::{badge_state, memory_reason, separate_fit_label, subtitle, tooltip},
};
use crate::types::{
    ActivityState, Compatibility,
    catalog::{CatalogFit, CatalogModel, CatalogVariant, SizeKind},
};

/// Only a failed, cancelled or unreported pull releases a kept variant; a
/// completed one installed it.
#[test]
fn only_a_pull_that_installed_nothing_releases() {
    for state in [None, Some(ActivityState::Failed), Some(ActivityState::Cancelled)] {
        assert!(pull_ended_without_install(state), "{state:?}");
    }
    for state in [
        ActivityState::Completed,
        ActivityState::Queued,
        ActivityState::Running,
        ActivityState::Cancelling,
    ] {
        assert!(!pull_ended_without_install(Some(state)), "{state:?}");
    }
}

/// A variant's explanation follows the resolved variant's fit and size kind
/// rather than repeating a stale default reason from search.
#[test]
fn variant_reason_matches_resolved_fit_and_size_kind() {
    let model = CatalogModel {
        id: "demo/model".to_owned(),
        compatibility: Compatibility::Supported,
        memory_fit: CatalogFit::Fits,
        reason: "estimated weights and runtime reserve fit the current budget".to_owned(),
        metal_compatibility: Compatibility::Supported,
        cuda_compatibility: Compatibility::Supported,
        ..Default::default()
    };
    assert_eq!(memory_reason(&model, None), model.reason);

    let q8_exceeds = CatalogVariant {
        name: "Q8_0".to_owned(),
        size_kind: SizeKind::Exact,
        weight_bytes: Some(9 << 30),
        memory_fit: CatalogFit::DoesNotFit,
    };
    assert_eq!(badge_state(&model, Some(&q8_exceeds)), "does_not_fit");
    assert_eq!(
        subtitle(&model, Some(&q8_exceeds)),
        "weights and runtime reserve exceed the current memory budget"
    );
    assert_eq!(separate_fit_label(&model, Some(&q8_exceeds)), None);

    let q4_fits = CatalogVariant {
        name: "Q4_K_M".to_owned(),
        size_kind: SizeKind::Exact,
        weight_bytes: Some(5 << 30),
        memory_fit: CatalogFit::Fits,
    };
    assert_eq!(
        tooltip(&model, Some(&q4_fits)),
        "weights and runtime reserve fit the current budget"
    );

    // BERT GGUF: overall compatibility is `unsupported` while backend-specific
    // Metal/CUDA compatibility fields remain `unknown`.
    let bert_gguf = CatalogModel {
        id: "owner/bert-GGUF".to_owned(),
        compatibility: Compatibility::Unsupported,
        metal_compatibility: Compatibility::Unknown,
        cuda_compatibility: Compatibility::Unknown,
        memory_fit: CatalogFit::Fits,
        reason: "model contract or weight format is not supported by libmir".to_owned(),
        ..Default::default()
    };
    assert_eq!(badge_state(&bert_gguf, Some(&q4_fits)), "unsupported");
    assert_eq!(subtitle(&bert_gguf, Some(&q4_fits)), bert_gguf.reason);
    assert_eq!(
        tooltip(&bert_gguf, Some(&q4_fits)),
        "model contract or weight format is not supported by libmir · weights and runtime reserve fit the current budget"
    );
    assert_eq!(separate_fit_label(&bert_gguf, Some(&q4_fits)).as_deref(), Some("fits"));
}

#[cfg(target_arch = "wasm32")]
mod browser;
