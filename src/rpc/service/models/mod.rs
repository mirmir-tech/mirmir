use libmir::{GenerationOverrides, Model, ProgressEvent};
use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

mod capability;
mod error;
mod events;
pub(super) mod residency;
mod stream;
#[cfg(test)]
mod tests;
pub use self::stream::stream_load;
use self::{capability::model_info, error::load as load_error};
pub(super) use self::{events::log_progress, residency::ModelResidency};
use super::status::lock;
pub struct ModelEntry {
    pub model: Model,
    pub info: proto::ModelInfo,
    last_used: u64,
}
impl RuntimeService {
    pub(super) fn load(
        &self,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry, Status> {
        let resolved = match self.store.resolve_model(selector) {
            Ok(resolved) => resolved,
            Err(error) => return Err(Status::invalid_argument(error.to_string())),
        };
        if let Some(model) = self.resident_model(&resolved.key)? {
            return Ok(model);
        }
        let memory_guard = lock(&self.model_memory_gate, "model memory gate")?;
        if let Some(model) = self.resident_model(&resolved.key)? {
            return Ok(model);
        }
        loop {
            match super::preflight::check(
                &self.library,
                &resolved.path,
                defaults(&resolved.generation),
                &resolved.key,
                force,
            ) {
                Ok(super::preflight::Check::Ready) => break,
                Ok(super::preflight::Check::Pressure { eviction_can_help: true, .. })
                    if self.evict_lru_model(&resolved.key)? => {},
                Ok(super::preflight::Check::Pressure { message, .. }) => {
                    return Err(Status::resource_exhausted(message));
                },
                Err(error) => return Err(error),
            }
        }
        let defaults = defaults(&resolved.generation);
        let mut loading = lock(&self.loading, "model lifecycle")?;
        if !loading.insert(resolved.key.clone()) {
            return Err(Status::failed_precondition("model is already loading"));
        }
        drop(loading);
        let path = resolved.path.display().to_string();
        let loaded = loop {
            let loaded = self.library.load_with_options(
                &resolved.path,
                defaults,
                libmir::ModelLoadOptions { allow_memory_overcommit: force },
                progress,
            );
            if matches!(&loaded, Err(libmir::Error::MemoryAdmission { .. }))
                && self.evict_lru_model(&resolved.key)?
            {
                continue;
            }
            break loaded;
        };
        if let Ok(model) = &loaded
            && let Err(error) = model.warm_execution_profiles(progress)
        {
            tracing::warn!(
                model = %resolved.key,
                %error,
                "accelerator profile warmup failed; retaining safe execution fallbacks"
            );
        }
        let mut loading = lock(&self.loading, "model lifecycle")?;
        loading.remove(&resolved.key);
        drop(loading);
        let loaded = match loaded {
            Ok(loaded) => loaded,
            Err(error) => return Err(load_error(&error)),
        };
        let entry = ModelEntry {
            info: model_info(resolved.key.clone(), path, &loaded),
            model: loaded,
            last_used: self.model_residency.next(),
        };
        let mut models = lock(&self.models, "model registry")?;
        models.insert(resolved.key, entry.clone());
        drop(models);
        if let Err(error) = self.store.activate_model(&entry.info.id) {
            tracing::warn!(%error, model = entry.info.id, "failed to persist active model");
        }
        if let Err(error) = self.store.remember_model(&entry.info.id) {
            tracing::warn!(%error, model = entry.info.id, "failed to persist recent model");
        }
        drop(memory_guard);
        Ok(entry)
    }

    pub(super) fn list(&self) -> Result<Vec<proto::ModelInfo>, Status> {
        let models = lock(&self.models, "model registry")?;
        let mut listed = models.values().map(|entry| entry.info.clone()).collect::<Vec<_>>();
        drop(models);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }

    pub(super) fn unload(&self, selector: &str) -> Result<bool, Status> {
        let memory_guard = lock(&self.model_memory_gate, "model memory gate")?;
        let key = self
            .store
            .resolve_model(selector)
            .map_or_else(|_| selector.to_owned(), |model| model.key);
        let mut models = lock(&self.models, "model registry")?;
        let entry = models.remove(selector).or_else(|| models.remove(&key));
        let Some(entry) = entry else {
            drop(models);
            if let Err(error) = self.store.deactivate_model(&key) {
                return Err(Status::internal(error.to_string()));
            }
            drop(memory_guard);
            tracing::info!(model = %key, unloaded = false, "model unload requested");
            return Ok(false);
        };
        if entry.model.is_in_use() {
            models.insert(entry.info.id.clone(), entry);
            return Err(Status::failed_precondition("model is currently serving a request"));
        }
        drop(models);
        if let Err(error) = entry.model.unload() {
            return Err(Status::internal(error.to_string()));
        }
        if let Err(error) = self.store.deactivate_model(&key) {
            return Err(Status::internal(error.to_string()));
        }
        drop(memory_guard);
        tracing::info!(model = %key, unloaded = true, "model unload requested");
        Ok(true)
    }
}

impl Clone for ModelEntry {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            info: self.info.clone(),
            last_used: self.last_used,
        }
    }
}

const fn defaults(generation: &crate::config::GenerationConfig) -> GenerationOverrides {
    GenerationOverrides {
        max_tokens: generation.max_tokens,
        min_tokens: None,
        ignore_eos: None,
        temperature: generation.temperature,
        top_p: generation.top_p,
        top_k: generation.top_k,
        repetition_penalty: generation.repetition_penalty,
    }
}
