use libmir::{GenerationOverrides, ProgressEvent};

use super::super::NativeRuntime;
use crate::{
    application::{Check, Error, ModelEntry, ModelInfo, Result, eviction_candidate},
    config::GenerationConfig,
};

impl NativeRuntime {
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
        let _memory = self.lifecycle.memory_gate()?;
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
        let _loading = self.lifecycle.begin_loading(&resolved.key)?;
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
        let loaded = loaded?;
        let entry = ModelEntry {
            info: ModelInfo::loaded(
                resolved.key.clone(),
                resolved.path.display().to_string(),
                &loaded,
            ),
            model: loaded,
            last_used: self.lifecycle.next_residency(),
        };
        self.lifecycle.state()?.resident.insert(resolved.key, entry.clone());
        if let Err(error) = self.store.activate_model(&entry.info.id) {
            tracing::warn!(%error, model = entry.info.id, "failed to persist active model");
        }
        if let Err(error) = self.store.remember_model(&entry.info.id) {
            tracing::warn!(%error, model = entry.info.id, "failed to persist recent model");
        }
        Ok(entry)
    }

    pub fn resident_model(&self, key: &str) -> Result<Option<ModelEntry>> {
        let mut state = self.lifecycle.state()?;
        let Some(entry) = state.resident.get_mut(key) else {
            return Ok(None);
        };
        entry.last_used = self.lifecycle.next_residency();
        let entry = entry.clone();
        drop(state);
        Ok(Some(entry))
    }

    pub fn evict_lru_model(&self, protected: &str) -> Result<bool> {
        let mut state = self.lifecycle.state()?;
        let candidate = eviction_candidate(
            protected,
            state
                .resident
                .iter()
                .map(|(key, entry)| (key.as_str(), entry.last_used, entry.model.is_in_use())),
        )
        .map(str::to_owned);
        let Some(key) = candidate else {
            return Ok(false);
        };
        let Some(entry) = state.resident.remove(&key) else {
            return Ok(false);
        };
        drop(state);

        let model_id = entry.info.id.clone();
        entry.model.unload()?;
        if let Err(error) = self.store.deactivate_model(&model_id) {
            tracing::warn!(%error, model = model_id, "failed to persist automatic model eviction");
        }
        tracing::info!(model = model_id, "evicted idle least-recently-used model");
        self.share_released_kv_memory();
        Ok(true)
    }

    pub fn unload_model(&self, selector: &str) -> Result<bool> {
        let _memory = self.lifecycle.memory_gate()?;
        let key = self
            .store
            .resolve_model(selector)
            .map_or_else(|_| selector.to_owned(), |model| model.key);
        let mut state = self.lifecycle.state()?;
        let entry = state.resident.remove(selector).or_else(|| state.resident.remove(&key));
        let Some(entry) = entry else {
            drop(state);
            self.store
                .deactivate_model(&key)
                .map_err(|error| Error::Persistence(error.to_string()))?;
            tracing::info!(model = %key, unloaded = false, "model unload requested");
            return Ok(false);
        };
        if entry.model.is_in_use() {
            state.resident.insert(entry.info.id.clone(), entry);
            return Err(Error::ModelInUse(key));
        }
        drop(state);
        entry.model.unload()?;
        self.store
            .deactivate_model(&key)
            .map_err(|error| Error::Persistence(error.to_string()))?;
        tracing::info!(model = %key, unloaded = true, "model unload requested");
        self.share_released_kv_memory();
        Ok(true)
    }

    /// Lets the remaining idle models grow their K/V caches into the memory
    /// an unloaded model released.
    fn share_released_kv_memory(&self) {
        match self.library.rebalance_kv_caches() {
            Ok(grown) if grown > 0 => tracing::info!(grown, "resident models grew K/V caches"),
            Ok(_) => {},
            Err(error) => tracing::warn!(%error, "K/V cache rebalance after unload failed"),
        }
    }
}

const fn generation_defaults(generation: &GenerationConfig) -> GenerationOverrides {
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
