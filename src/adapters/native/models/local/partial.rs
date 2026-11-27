//! Downloads in progress or interrupted in the Hub cache: the GGUF
//! variants downloading into a repository's cache, else the repository.

use super::known::Repositories;
use crate::{
    application::{LocalModelInfo, LocalModelState},
    catalog::{
        PartialDownload, cached_repository_bytes, discover_partial_downloads_in, pending_bytes,
    },
    config::{HubReference, Paths, model_key},
};

/// The GGUF variants downloading, each from its repository's cache, then
/// the other repositories' partial caches. A repository with a variant
/// downloading lists as that variant alone, until the variant's
/// configuration is saved: its cache, complete or not, is not a model of
/// its own, and resuming it must not pull every quantization.
pub(super) fn append(
    listed: &mut Vec<LocalModelInfo>,
    paths: &Paths,
    pending: Vec<HubReference>,
    known: &mut Repositories,
) {
    for variant in pending {
        let repository = variant.repository().to_owned();
        // Its own files; a marker from an earlier version names none.
        let downloaded_bytes = pending_bytes(paths, &variant)
            .or_else(|| cached_repository_bytes(&paths.hub_cache_dir, &repository));
        if let Some(downloaded_bytes) = downloaded_bytes {
            let partial = PartialDownload {
                repo_id: variant.to_string(),
                downloaded_bytes,
            };
            append_one(listed, &partial, known);
        }
        known.insert(&repository);
    }
    for partial in discover_partial_downloads_in(&paths.hub_cache_dir) {
        append_one(listed, &partial, known);
    }
}

fn append_one(
    listed: &mut Vec<LocalModelInfo>,
    partial: &PartialDownload,
    known: &mut Repositories,
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
    known.insert(&partial.repo_id);
}
