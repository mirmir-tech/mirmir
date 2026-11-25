//! Downloads interrupted in the Hub cache. A repository's partial cache
//! lists as the GGUF variants downloading into it, else as the repository.

use std::collections::HashSet;

use crate::{
    application::{LocalModelInfo, LocalModelState},
    catalog::{PartialDownload, discover_partial_downloads_in, pending_variants},
    config::{Paths, model_key},
};

pub(super) fn append(listed: &mut Vec<LocalModelInfo>, paths: &Paths, known: &mut HashSet<String>) {
    for partial in discover_partial_downloads_in(&paths.hub_cache_dir) {
        // Resuming a variant must not pull the whole repository.
        let variants = pending_variants(paths, &partial.repo_id);
        if variants.is_empty() {
            append_one(listed, partial, known);
            continue;
        }
        for variant in variants {
            let partial = PartialDownload {
                repo_id: variant.to_string(),
                ..partial.clone()
            };
            append_one(listed, partial, known);
        }
    }
}

fn append_one(
    listed: &mut Vec<LocalModelInfo>,
    partial: PartialDownload,
    known: &mut HashSet<String>,
) {
    if known.contains(&partial.repo_id) {
        return;
    }
    let Ok(id) = model_key(&partial.repo_id) else {
        return;
    };
    listed.push(LocalModelInfo {
        id,
        repo_id: partial.repo_id.clone(),
        revision: "main".to_owned(),
        commit: String::new(),
        path: String::new(),
        state: LocalModelState::Paused,
        recent_rank: None,
        selector: partial.repo_id.clone(),
        managed: true,
        image_input: false,
        image_unavailable_reason: String::new(),
        model_class: String::new(),
        loadable: false,
        load_unavailable_reason: "Download incomplete; resume or remove it".to_owned(),
        ecosystem: crate::model::format::Ecosystem::Unknown,
        container: None,
        encoding: crate::model::format::ModelEncoding::Unknown,
        metal_compatibility: libmir::AdmissionStatus::Unknown,
        cuda_compatibility: libmir::AdmissionStatus::Unknown,
        size_bytes: partial.downloaded_bytes,
        tool_use: false,
        thinking: false,
        vision: false,
    });
    known.insert(partial.repo_id);
}
