use std::collections::{HashMap, HashSet};

use libmir::{GenerationOverrides, ModelDescriptor, models::layout::ModelMetadata};
use tonic::Status;

use super::RuntimeService;
use crate::{
    catalog::{CachedModel, discover_cached_models, discover_cached_models_in},
    config::{GenerationConfig, HubModelConfig, ModelConfig, model_key},
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
    let selector = cached.snapshot.display().to_string();
    let state = state(&selector, &cached.snapshot, models, loading);
    let load = loadability(&cached.snapshot, GenerationConfig::default());
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
        model_class: model_class(&cached.snapshot),
        loadable: load.allowed,
        load_unavailable_reason: load.reason,
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
    let load = loadability(&config.path, config.generation);
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
        image_input: models.get(&config.id).is_some_and(|entry| entry.info.image_input),
        image_unavailable_reason: models
            .get(&config.id)
            .map(|entry| entry.info.image_unavailable_reason.clone())
            .unwrap_or_default(),
        model_class: model_class(&config.path),
        loadable: load.allowed,
        load_unavailable_reason: load.reason,
    }
}

struct Loadability {
    allowed: bool,
    reason: String,
}

fn loadability(path: &std::path::Path, generation: GenerationConfig) -> Loadability {
    if !path.exists() {
        return Loadability {
            allowed: false,
            reason: format!("model files are missing at {}", path.display()),
        };
    }
    let overrides = GenerationOverrides {
        max_tokens: generation.max_tokens,
        temperature: generation.temperature,
        top_p: generation.top_p,
        top_k: generation.top_k,
        repetition_penalty: generation.repetition_penalty,
    };
    match ModelDescriptor::inspect(path, overrides) {
        Ok(_) => Loadability { allowed: true, reason: String::new() },
        Err(error) => Loadability {
            allowed: false,
            reason: error.to_string(),
        },
    }
}

fn model_class(path: &std::path::Path) -> String {
    ModelMetadata::from_config_path(path.join("config.json")).map_or_else(
        |_| "unknown".to_owned(),
        |metadata| {
            if metadata.architectures.is_empty() {
                "unknown".to_owned()
            } else {
                metadata.architectures.join(", ")
            }
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, Paths, Store};

    #[test]
    fn lists_an_unconfigured_managed_snapshot_with_its_load_error() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "mirmir-local-unsupported-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("model")
        ));
        let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
        let snapshot = paths.hub_cache_dir.join("models--Org--Unsupported/snapshots/deadbeef");
        std::fs::create_dir_all(&snapshot)?;
        std::fs::write(snapshot.join("config.json"), "{}")?;
        let service = RuntimeService::new(&AppConfig::default(), Store::new(paths));

        let models = service.local_models().expect("local models should be listed");
        let model = models
            .iter()
            .find(|model| model.repo_id == "Org/Unsupported")
            .expect("managed snapshot should remain visible");
        assert!(model.managed);
        assert_eq!(model.state, "available");
        assert!(!model.loadable);
        assert!(!model.load_unavailable_reason.is_empty());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
