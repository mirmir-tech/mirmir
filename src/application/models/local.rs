use std::collections::{HashMap, HashSet};

use super::{LocalModelInfo, ModelEntry, presentation};
use crate::{
    application::{Application, Result, RuntimeCoordinator},
    catalog::{
        CachedModel, PartialDownload, discover_cached_models, discover_cached_models_in,
        discover_partial_downloads_in,
    },
    config::{GenerationConfig, HubModelConfig, ModelConfig, model_key},
};

impl Application {
    pub fn local_models(&self) -> Result<Vec<LocalModelInfo>> {
        self.runtime.local_models()
    }
}

impl RuntimeCoordinator {
    pub fn local_models(&self) -> Result<Vec<LocalModelInfo>> {
        let configs = self.store.list_model_configs()?;
        let recent = self
            .store
            .recent_models()?
            .into_iter()
            .enumerate()
            .map(|(rank, id)| (id, u32::try_from(rank).unwrap_or(u32::MAX)))
            .collect::<HashMap<_, _>>();
        let state = self.lifecycle.state()?;
        let mut known_repos = configs
            .iter()
            .filter_map(|config| config.hub.as_ref().map(|hub| hub.repo_id.clone()))
            .collect::<HashSet<_>>();
        let mut known_paths =
            configs.iter().map(|config| config.path.clone()).collect::<HashSet<_>>();
        let mut listed = configs
            .iter()
            .map(|config| configured(config, &recent, &state.resident, &state.loading, self))
            .collect::<Vec<_>>();
        for cached in discover_cached_models_in(&self.store.paths().hub_cache_dir) {
            append_cached(
                &mut listed, cached, true, &mut known_repos, &mut known_paths, &state.resident,
                &state.loading,
            );
        }
        for partial in discover_partial_downloads_in(&self.store.paths().hub_cache_dir) {
            append_partial(&mut listed, partial, &mut known_repos);
        }
        for cached in discover_cached_models() {
            append_cached(
                &mut listed, cached, false, &mut known_repos, &mut known_paths, &state.resident,
                &state.loading,
            );
        }
        drop(state);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }
}

fn append_partial(
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
    known.insert(partial.repo_id);
}

fn append_cached(
    listed: &mut Vec<LocalModelInfo>,
    cached: CachedModel,
    managed: bool,
    known_repos: &mut HashSet<String>,
    known_paths: &mut HashSet<std::path::PathBuf>,
    models: &HashMap<String, ModelEntry>,
    loading: &HashSet<String>,
) {
    if known_repos.contains(&cached.repo_id) || known_paths.contains(&cached.snapshot) {
        return;
    }
    let Ok(id) = model_key(&cached.repo_id) else {
        return;
    };
    let details = presentation::inspect(&cached.snapshot, GenerationConfig::default());
    let selector = cached.snapshot.display().to_string();
    listed.push(info(
        id,
        cached.repo_id.clone(),
        cached.revision,
        cached.commit,
        selector.clone(),
        state(&selector, &cached.snapshot, details.loadable, models, loading),
        selector,
        managed,
        None,
        &details,
        None,
    ));
    known_repos.insert(cached.repo_id);
    known_paths.insert(cached.snapshot);
}

fn configured(
    config: &ModelConfig,
    recent: &HashMap<String, u32>,
    models: &HashMap<String, ModelEntry>,
    loading: &HashSet<String>,
    coordinator: &RuntimeCoordinator,
) -> LocalModelInfo {
    let hub = config.hub.clone().unwrap_or_else(|| HubModelConfig {
        repo_id: String::new(),
        revision: String::new(),
        commit: String::new(),
    });
    let details = presentation::inspect(&config.path, config.generation);
    info(
        config.id.clone(),
        hub.repo_id,
        hub.revision,
        hub.commit,
        config.path.display().to_string(),
        state(&config.id, &config.path, details.loadable, models, loading),
        config.id.clone(),
        config.hub.is_some() && config.path.starts_with(&coordinator.store.paths().hub_cache_dir),
        recent.get(&config.id).copied(),
        &details,
        models.get(&config.id),
    )
}

#[allow(clippy::too_many_arguments)]
fn info(
    id: String,
    repo_id: String,
    revision: String,
    commit: String,
    path: String,
    state: &str,
    selector: String,
    managed: bool,
    recent_rank: Option<u32>,
    details: &presentation::ModelPresentation,
    loaded: Option<&ModelEntry>,
) -> LocalModelInfo {
    LocalModelInfo {
        id,
        repo_id,
        revision,
        commit,
        path,
        state: state.to_owned(),
        recent_rank,
        selector,
        managed,
        image_input: loaded.is_some_and(|entry| entry.info.image_input),
        image_unavailable_reason: loaded
            .map(|entry| entry.info.image_unavailable_reason.clone())
            .unwrap_or_default(),
        model_class: details.class.clone(),
        loadable: details.loadable,
        load_unavailable_reason: details.error.clone(),
        library: details.library.clone(),
        ecosystem: details.ecosystem.clone(),
        container: details.container.clone(),
        encoding: details.encoding.clone(),
        metal_compatibility: details.metal_compatibility.clone(),
        cuda_compatibility: details.cuda_compatibility.clone(),
        size_bytes: details.size_bytes,
        tool_use: details.features.tool_use,
        thinking: details.features.thinking,
        vision: details.features.vision,
    }
}

fn state<'a>(
    selector: &str,
    path: &std::path::Path,
    loadable: bool,
    models: &HashMap<String, ModelEntry>,
    loading: &HashSet<String>,
) -> &'a str {
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
