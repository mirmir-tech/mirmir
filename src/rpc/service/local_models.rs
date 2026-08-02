use std::collections::{HashMap, HashSet};

use tonic::Status;

use super::RuntimeService;
use crate::{
    catalog::{
        CachedModel, PartialDownload, discover_cached_models, discover_cached_models_in,
        discover_partial_downloads_in,
    },
    config::{GenerationConfig, HubModelConfig, ModelConfig, model_key},
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn local_models(&self) -> Result<Vec<proto::LocalModelInfo>, Status> {
        let configs = super::status::internal(self.store.list_model_configs())?;
        let recent = super::status::internal(self.store.recent_models())?
            .into_iter()
            .enumerate()
            .map(|(rank, id)| (id, u32::try_from(rank).unwrap_or(u32::MAX)))
            .collect::<HashMap<_, _>>();
        let models = super::status::lock(&self.models, "model registry")?;
        let loading = super::status::lock(&self.loading, "model lifecycle")?;
        let mut known_repos = configs
            .iter()
            .filter_map(|config| config.hub.as_ref().map(|hub| hub.repo_id.clone()))
            .collect::<HashSet<_>>();
        let mut known_paths =
            configs.iter().map(|config| config.path.clone()).collect::<HashSet<_>>();
        let mut listed = configs
            .into_iter()
            .map(|config| configured(&config, &recent, &models, &loading, self))
            .collect::<Vec<_>>();
        for cached in discover_cached_models_in(&self.store.paths().hub_cache_dir) {
            append_cached(
                &mut listed, cached, true, &mut known_repos, &mut known_paths, &models, &loading,
            );
        }
        for partial in discover_partial_downloads_in(&self.store.paths().hub_cache_dir) {
            append_partial(&mut listed, partial, &mut known_repos);
        }
        for cached in discover_cached_models() {
            append_cached(
                &mut listed, cached, false, &mut known_repos, &mut known_paths, &models, &loading,
            );
        }
        drop(loading);
        drop(models);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }
}

fn append_partial(
    listed: &mut Vec<proto::LocalModelInfo>,
    partial: PartialDownload,
    known_repos: &mut HashSet<String>,
) {
    if known_repos.contains(&partial.repo_id) {
        return;
    }
    let Ok(id) = model_key(&partial.repo_id) else {
        return;
    };
    listed.push(proto::LocalModelInfo {
        id,
        repo_id: partial.repo_id.clone(),
        revision: "main".to_owned(),
        commit: String::new(),
        path: String::new(),
        state: "paused".to_owned(),
        recent_rank: None,
        selector: partial.repo_id.clone(),
        managed: true,
        image_input: false,
        image_unavailable_reason: String::new(),
        model_class: String::new(),
        loadable: false,
        load_unavailable_reason: "Download incomplete; resume or remove it".to_owned(),
        library: "Unknown".to_owned(),
        ecosystem: "Unknown".to_owned(),
        container: "Unknown".to_owned(),
        encoding: "Unknown".to_owned(),
        metal_compatibility: "unknown".to_owned(),
        cuda_compatibility: "unknown".to_owned(),
        size_bytes: partial.downloaded_bytes,
        tool_use: false,
        thinking: false,
        vision: false,
    });
    known_repos.insert(partial.repo_id);
}

fn append_cached(
    listed: &mut Vec<proto::LocalModelInfo>,
    cached: CachedModel,
    managed: bool,
    known_repos: &mut HashSet<String>,
    known_paths: &mut HashSet<std::path::PathBuf>,
    models: &HashMap<String, super::models::ModelEntry>,
    loading: &HashSet<String>,
) {
    if known_repos.contains(&cached.repo_id) || known_paths.contains(&cached.snapshot) {
        return;
    }
    let Ok(id) = model_key(&cached.repo_id) else {
        return;
    };
    let details = super::presentation::inspect(&cached.snapshot, GenerationConfig::default());
    let selector = cached.snapshot.display().to_string();
    let state = state(&selector, &cached.snapshot, details.loadable, models, loading);
    listed.push(proto::LocalModelInfo {
        id,
        repo_id: cached.repo_id.clone(),
        revision: cached.revision,
        commit: cached.commit,
        path: selector.clone(),
        state: state.to_owned(),
        recent_rank: None,
        selector,
        managed,
        image_input: false,
        image_unavailable_reason: String::new(),
        model_class: details.class,
        loadable: details.loadable,
        load_unavailable_reason: details.error,
        library: details.library,
        ecosystem: details.ecosystem,
        container: details.container,
        encoding: details.encoding,
        metal_compatibility: details.metal_compatibility,
        cuda_compatibility: details.cuda_compatibility,
        size_bytes: details.size_bytes,
        tool_use: details.features.tool_use,
        thinking: details.features.thinking,
        vision: details.features.vision,
    });
    known_repos.insert(cached.repo_id);
    known_paths.insert(cached.snapshot);
}

fn configured(
    config: &ModelConfig,
    recent: &HashMap<String, u32>,
    models: &HashMap<String, super::models::ModelEntry>,
    loading: &HashSet<String>,
    service: &RuntimeService,
) -> proto::LocalModelInfo {
    let hub = config.hub.clone().unwrap_or_else(|| HubModelConfig {
        repo_id: String::new(),
        revision: String::new(),
        commit: String::new(),
    });
    let details = super::presentation::inspect(&config.path, config.generation);
    proto::LocalModelInfo {
        recent_rank: recent.get(&config.id).copied(),
        id: config.id.clone(),
        repo_id: hub.repo_id,
        revision: hub.revision,
        commit: hub.commit,
        path: config.path.display().to_string(),
        state: state(&config.id, &config.path, details.loadable, models, loading).to_owned(),
        selector: config.id.clone(),
        managed: config.hub.is_some()
            && config.path.starts_with(&service.store.paths().hub_cache_dir),
        image_input: models.get(&config.id).is_some_and(|entry| entry.info.image_input),
        image_unavailable_reason: models
            .get(&config.id)
            .map(|entry| entry.info.image_unavailable_reason.clone())
            .unwrap_or_default(),
        model_class: details.class,
        loadable: details.loadable,
        load_unavailable_reason: details.error,
        library: details.library,
        ecosystem: details.ecosystem,
        container: details.container,
        encoding: details.encoding,
        metal_compatibility: details.metal_compatibility,
        cuda_compatibility: details.cuda_compatibility,
        size_bytes: details.size_bytes,
        tool_use: details.features.tool_use,
        thinking: details.features.thinking,
        vision: details.features.vision,
    }
}

fn state(
    selector: &str,
    path: &std::path::Path,
    loadable: bool,
    models: &HashMap<String, super::models::ModelEntry>,
    loading: &HashSet<String>,
) -> &'static str {
    if models.contains_key(selector) {
        "ready"
    } else if loading.contains(selector) {
        "loading"
    } else if !loadable {
        "error"
    } else if path.exists() {
        "available"
    } else {
        "missing"
    }
}

#[cfg(test)]
mod tests;
