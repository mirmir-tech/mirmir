use std::collections::{HashMap, HashSet};

use tonic::Status;

use super::RuntimeService;
use crate::{
    catalog::discover_cached_models,
    config::{HubModelConfig, ModelConfig, model_key},
    rpc::proto,
};

impl RuntimeService {
    pub(super) fn local_models(&self) -> Result<Vec<proto::LocalModelInfo>, Status> {
        let configs = self
            .store
            .list_model_configs()
            .map_err(|error| Status::internal(error.to_string()))?;
        let recent = self
            .store
            .recent_models()
            .map_err(|error| Status::internal(error.to_string()))?
            .into_iter()
            .enumerate()
            .map(|(rank, id)| (id, u32::try_from(rank).unwrap_or(u32::MAX)))
            .collect::<HashMap<_, _>>();
        let models = self
            .models
            .lock()
            .map_err(|_| Status::internal("model registry lock is poisoned"))?;
        let loading = self
            .loading
            .lock()
            .map_err(|_| Status::internal("model lifecycle lock is poisoned"))?;
        let known_repos = configs
            .iter()
            .filter_map(|config| config.hub.as_ref().map(|hub| hub.repo_id.clone()))
            .collect::<HashSet<_>>();
        let known_paths = configs.iter().map(|config| config.path.clone()).collect::<HashSet<_>>();
        let mut listed = configs
            .into_iter()
            .map(|config| configured(&config, &recent, &models, &loading, self))
            .collect::<Vec<_>>();
        for cached in discover_cached_models() {
            if known_repos.contains(&cached.repo_id) || known_paths.contains(&cached.snapshot) {
                continue;
            }
            let Ok(id) = model_key(&cached.repo_id) else {
                continue;
            };
            let selector = cached.snapshot.display().to_string();
            let state = state(&selector, &cached.snapshot, &models, &loading);
            listed.push(proto::LocalModelInfo {
                id,
                repo_id: cached.repo_id,
                revision: cached.revision,
                commit: cached.commit,
                path: selector.clone(),
                state: state.to_owned(),
                recent_rank: None,
                selector,
                managed: false,
            });
        }
        drop(loading);
        drop(models);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }
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
    proto::LocalModelInfo {
        recent_rank: recent.get(&config.id).copied(),
        id: config.id.clone(),
        repo_id: hub.repo_id,
        revision: hub.revision,
        commit: hub.commit,
        path: config.path.display().to_string(),
        state: state(&config.id, &config.path, models, loading).to_owned(),
        selector: config.id.clone(),
        managed: config.hub.is_some()
            && config.path.starts_with(&service.store.paths().hub_cache_dir),
    }
}

fn state(
    selector: &str,
    path: &std::path::Path,
    models: &HashMap<String, super::models::ModelEntry>,
    loading: &HashSet<String>,
) -> &'static str {
    if models.contains_key(selector) {
        "ready"
    } else if loading.contains(selector) {
        "loading"
    } else if path.exists() {
        "available"
    } else {
        "missing"
    }
}
