use libmir::TensorCatalog;

use super::*;
use crate::catalog::preflight::RemoteModelMetadata;

#[test]
fn cache_evicts_the_least_recently_used_commit() {
    let mut state = State::default();
    for index in 0..MAX_ENTRIES {
        state.insert(key("Org/Model", &format!("revision-{index}")), preflight());
    }
    let first = key("Org/Model", "revision-0");
    let first_value = state.entries.get(&first).cloned();
    assert!(first_value.is_some());
    state.order.retain(|candidate| candidate != &first);
    state.order.push_back(first.clone());

    state.insert(key("Org/Model", "newest"), preflight());

    assert!(state.entries.contains_key(&first));
    assert!(!state.entries.contains_key(&key("Org/Model", "revision-1")));
    assert_eq!(state.entries.len(), MAX_ENTRIES);
}

fn preflight() -> RemoteHeaderPreflight {
    RemoteHeaderPreflight {
        catalog: TensorCatalog { tensors: Vec::new() },
        metadata: RemoteModelMetadata {
            config: serde_json::json!({}),
            tokenizer_config: None,
            modules: None,
            pooling: None,
            sentence_transformers: None,
            processor_config: None,
            tokenizer_assets: super::super::tests::tokenizer_assets(),
        },
        contract: None,
        contract_error: None,
        fetched_bytes: 0,
        files: Vec::new(),
        revision: "revision".into(),
    }
}
