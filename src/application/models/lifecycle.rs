use libmir::{GenerationOverrides, ProgressEvent};

use super::{ModelEntry, preflight::Check, residency::eviction_candidate};
use crate::application::{Error, Result, RuntimeCoordinator};

impl RuntimeCoordinator {
    pub fn load_model(
        &self,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry> {
        let resolved = self
            .store
            .resolve_model(selector)
            .map_err(|error| Error::InvalidModel(error.to_string()))?;
        if let Some(model) = self.resident_model(&resolved.key)? {
            return Ok(model);
        }
        let _memory = self
            .model_memory_gate
            .lock()
            .map_err(|_| Error::StatePoisoned("model memory gate"))?;
        if let Some(model) = self.resident_model(&resolved.key)? {
            return Ok(model);
        }
        loop {
            match self.preflight(
                &resolved.path,
                generation_defaults(&resolved.generation),
                &resolved.key,
                force,
            )? {
                Check::Ready => break,
                Check::Pressure { eviction_can_help: true, .. }
                    if self.evict_lru_model(&resolved.key)? => {},
                Check::Pressure { message, .. } => return Err(Error::MemoryPressure(message)),
            }
        }
        self.begin_loading(&resolved.key)?;
        let defaults = generation_defaults(&resolved.generation);
        let loaded = loop {
            let attempt = self.library.load_with_options(
                &resolved.path,
                defaults,
                libmir::ModelLoadOptions { allow_memory_overcommit: force },
                progress,
            );
            if matches!(&attempt, Err(libmir::Error::MemoryAdmission { .. })) {
                match self.evict_lru_model(&resolved.key) {
                    Ok(true) => continue,
                    Ok(false) => {},
                    Err(error) => break Err(error),
                }
            }
            break attempt.map_err(Error::from);
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
        self.finish_loading(&resolved.key)?;
        let loaded = loaded?;
        let entry = ModelEntry {
            info: super::ModelInfo::loaded(
                resolved.key.clone(),
                resolved.path.display().to_string(),
                &loaded,
            ),
            model: loaded,
            last_used: self.model_residency.next(),
        };
        let mut models = self.models.lock().map_err(|_| Error::StatePoisoned("model registry"))?;
        models.insert(resolved.key, entry.clone());
        drop(models);
        if let Err(error) = self.store.activate_model(&entry.info.id) {
            tracing::warn!(%error, model = entry.info.id, "failed to persist active model");
        }
        if let Err(error) = self.store.remember_model(&entry.info.id) {
            tracing::warn!(%error, model = entry.info.id, "failed to persist recent model");
        }
        Ok(entry)
    }

    pub fn resident_model(&self, key: &str) -> Result<Option<ModelEntry>> {
        let mut models = self.models.lock().map_err(|_| Error::StatePoisoned("model registry"))?;
        let Some(entry) = models.get_mut(key) else {
            return Ok(None);
        };
        entry.last_used = self.model_residency.next();
        let entry = entry.clone();
        drop(models);
        Ok(Some(entry))
    }

    pub fn evict_lru_model(&self, protected: &str) -> Result<bool> {
        let mut models = self.models.lock().map_err(|_| Error::StatePoisoned("model registry"))?;
        let candidate = eviction_candidate(
            protected,
            models
                .iter()
                .map(|(key, entry)| (key.as_str(), entry.last_used, entry.model.is_in_use())),
        )
        .map(str::to_owned);
        let Some(key) = candidate else {
            return Ok(false);
        };
        let Some(entry) = models.remove(&key) else {
            return Ok(false);
        };
        drop(models);

        let model_id = entry.info.id.clone();
        entry.model.unload()?;
        if let Err(error) = self.store.deactivate_model(&model_id) {
            tracing::warn!(%error, model = model_id, "failed to persist automatic model eviction");
        }
        tracing::info!(model = model_id, "evicted idle least-recently-used model");
        Ok(true)
    }

    pub fn unload_model(&self, selector: &str) -> Result<bool> {
        let _memory = self
            .model_memory_gate
            .lock()
            .map_err(|_| Error::StatePoisoned("model memory gate"))?;
        let key = self
            .store
            .resolve_model(selector)
            .map_or_else(|_| selector.to_owned(), |model| model.key);
        let mut models = self.models.lock().map_err(|_| Error::StatePoisoned("model registry"))?;
        let entry = models.remove(selector).or_else(|| models.remove(&key));
        let Some(entry) = entry else {
            drop(models);
            self.store
                .deactivate_model(&key)
                .map_err(|error| Error::Persistence(error.to_string()))?;
            tracing::info!(model = %key, unloaded = false, "model unload requested");
            return Ok(false);
        };
        if entry.model.is_in_use() {
            models.insert(entry.info.id.clone(), entry);
            return Err(Error::ModelInUse(key));
        }
        drop(models);
        entry.model.unload()?;
        self.store
            .deactivate_model(&key)
            .map_err(|error| Error::Persistence(error.to_string()))?;
        tracing::info!(model = %key, unloaded = true, "model unload requested");
        Ok(true)
    }

    fn begin_loading(&self, key: &str) -> Result<()> {
        let mut loading =
            self.loading.lock().map_err(|_| Error::StatePoisoned("model lifecycle"))?;
        if !loading.insert(key.to_owned()) {
            return Err(Error::ModelAlreadyLoading(key.to_owned()));
        }
        drop(loading);
        Ok(())
    }

    fn finish_loading(&self, key: &str) -> Result<()> {
        let mut loading =
            self.loading.lock().map_err(|_| Error::StatePoisoned("model lifecycle"))?;
        loading.remove(key);
        drop(loading);
        Ok(())
    }
}

const fn generation_defaults(generation: &crate::config::GenerationConfig) -> GenerationOverrides {
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
