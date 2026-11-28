use super::*;
use crate::types::{
    Compatibility, ModelState,
    catalog::{CatalogFit, CatalogModel, CatalogVariant, SizeKind},
};

fn local(repo_id: &str, state: ModelState) -> Model {
    Model {
        repo_id: repo_id.to_owned(),
        state,
        ..Default::default()
    }
}

#[test]
fn a_listed_model_is_downloaded_in_any_case() {
    let local = [local("Qwen/Test", ModelState::Available)];
    assert!(installed(&local, "Qwen/Test") && installed(&local, "qwen/test"));
    assert!(!installed(&local, "Qwen/Other"));
}

/// A variant is downloaded by its own `id:name`, not its repository's
/// or another variant's.
#[test]
fn a_downloaded_variant_is_its_own() {
    let local = [local("org/m-GGUF:Q8_0", ModelState::Ready)];
    assert!(installed(&local, "org/m-GGUF:q8_0"));
    assert!(!installed(&local, "org/m-GGUF:Q4_K_M") && !installed(&local, "org/m-GGUF"));
}

/// Only whole files count: a paused, missing or unknown state, or a
/// download this dashboard started, is not downloaded; a model on disk
/// that does not load is.
#[test]
fn an_incomplete_model_is_not_downloaded() {
    for state in
        [ModelState::Paused, ModelState::Missing, ModelState::Downloading, ModelState::Unknown]
    {
        assert!(!installed(&[local("Qwen/Test", state)], "Qwen/Test"), "{state:?}");
    }
    for state in ["ready", "loading", "available", "error"] {
        let parsed = serde_json::from_value::<ModelState>(serde_json::json!(state));
        assert!(parsed.is_ok_and(ModelState::installed), "{state}");
    }
    let future = serde_json::from_value::<ModelState>(serde_json::json!("someday"));
    assert!(future.is_ok_and(|state| state == ModelState::Unknown));
}

/// A GGUF repository stays visible under the compatible filter whenever any
/// offered variant fits, even when its default variant does not.
#[test]
fn gguf_row_fits_when_any_offered_variant_fits() {
    let variant = |name: &str, fit: CatalogFit| CatalogVariant {
        name: name.to_owned(),
        size_kind: SizeKind::Estimated,
        weight_bytes: Some(4 << 30),
        memory_fit: fit,
    };
    let mixed = CatalogModel {
        id: "demo/mixed-GGUF".to_owned(),
        memory_fit: CatalogFit::DoesNotFit,
        default_variant: Some("Q4_K_M".to_owned()),
        variants: vec![
            variant("Q2_K", CatalogFit::Fits),
            variant("Q4_K_M", CatalogFit::DoesNotFit),
        ],
        ..Default::default()
    };
    assert!(row_fits(&mixed));

    let none_fit = CatalogModel {
        variants: vec![
            variant("Q2_K", CatalogFit::DoesNotFit),
            variant("Q4_K_M", CatalogFit::DoesNotFit),
        ],
        ..mixed
    };
    assert!(!row_fits(&none_fit));

    let plain_too_large = CatalogModel {
        id: "demo/plain".to_owned(),
        memory_fit: CatalogFit::DoesNotFit,
        ..Default::default()
    };
    assert!(!row_fits(&plain_too_large));

    let bert_gguf = CatalogModel {
        id: "owner/bert-GGUF".to_owned(),
        compatibility: Compatibility::Unsupported,
        metal_compatibility: Compatibility::Unknown,
        cuda_compatibility: Compatibility::Unknown,
        memory_fit: CatalogFit::Fits,
        variants: vec![variant("Q8_0", CatalogFit::Fits)],
        ..Default::default()
    };
    assert!(unsupported(&bert_gguf));
    assert!(row_fits(&bert_gguf));
}

#[cfg(target_arch = "wasm32")]
mod browser;
pub(super) mod probe;
