use super::*;
use crate::types::ModelState;

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

#[cfg(target_arch = "wasm32")]
mod browser;
