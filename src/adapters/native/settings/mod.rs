mod capabilities;

use libmir::{GenerationOverrides, Model, ModelDescriptor, ModelLoadOptions, ModelTask};

use self::capabilities::{capabilities, task_name};
use super::NativeRuntime;
use crate::{
    application::{
        ContextChoice, ContextLimits, LoadDefaults, ModelContext, ModelDefaults, ModelInspection,
        Result, fitting_tokens,
    },
    config::{
        GenerationConfig, HubModelConfig, ModelConfigChange, ModelRuntimeConfig, folder_key,
        model_key,
    },
};

impl NativeRuntime {
    pub fn inspect_model(
        &self,
        selector: &str,
        context: Option<ContextChoice>,
    ) -> Result<ModelInspection> {
        let resolved = self.store.resolve_model(selector)?;
        let descriptor = ModelDescriptor::inspect(&resolved.path, overrides(resolved.generation))?;
        // A loaded copy is replaced by a reload, so its memory counts as free.
        let resident = self.peek_resident_model(&resolved.key)?;
        let limits = self.context_limits(&descriptor);
        let saved = limits.choice(resolved.runtime.kv_blocks);
        let shown = context.unwrap_or(saved);
        let mut memory =
            self.memory_report(&descriptor, options(limits.blocks(shown)?), resident.as_ref())?;
        // A chosen context is a shared cache: report the largest that fits,
        // up to what the model's requests can use, not one request's context.
        memory.max_safe_context = memory
            .budget
            .and_then(|budget| fitting_tokens(memory.estimate, budget))
            .map(|tokens| tokens.min(limits.max_tokens));
        let model_context = (memory.estimate.kv_bytes_per_token > 0)
            .then(|| -> Result<ModelContext> {
                let automatic_tokens = if shown == ContextChoice::Automatic {
                    memory.estimate.cache_capacity_tokens
                } else {
                    self.library
                        .preview_load(&descriptor, options(None), resident.as_ref())?
                        .estimate
                        .cache_capacity_tokens
                };
                Ok(ModelContext {
                    limits,
                    saved,
                    automatic_tokens,
                    resident_tokens: resident.as_ref().map(resident_tokens),
                })
            })
            .transpose()?;
        Ok(ModelInspection {
            settings: matches!(descriptor.task(), ModelTask::Generation)
                .then(|| descriptor.generation()),
            has_mirmir_overrides: resolved.generation.has_overrides(),
            memory,
            task: task_name(&descriptor.task()),
            capabilities: capabilities(&descriptor),
            context: model_context,
        })
    }

    pub fn prepare_load(&self, selector: &str, load: LoadDefaults) -> Result<String> {
        if load.defaults.generation.is_none() && load.defaults.context.is_none() {
            return Ok(selector.to_owned());
        }
        self.save_defaults(selector, &load.config_id, load.hub, load.defaults)
    }

    pub fn save_model_defaults(&self, selector: &str, defaults: ModelDefaults) -> Result<String> {
        self.save_defaults(selector, "", None, defaults)
    }

    fn save_defaults(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
        defaults: ModelDefaults,
    ) -> Result<String> {
        let (config_id, hub) = self.identity(selector, config_id, hub)?;
        let resolved = self.store.resolve_model(selector)?;
        let generation = defaults.generation.unwrap_or(resolved.generation);
        let descriptor = ModelDescriptor::inspect(&resolved.path, overrides(generation))?;
        let runtime = defaults
            .context
            .map(|choice| -> Result<ModelRuntimeConfig> {
                Ok(ModelRuntimeConfig {
                    kv_blocks: self.context_limits(&descriptor).blocks(choice)?,
                })
            })
            .transpose()?;
        let change = ModelConfigChange { generation: defaults.generation, runtime };
        Ok(self.store.save_model_config(selector, &config_id, hub, change)?)
    }

    /// The configuration a model's defaults are saved under. A configured
    /// model keeps its own; a cached checkpoint named only by its path, as
    /// clients name what the server lists, is saved under the repository it
    /// was downloaded from, and any other folder under an id of its own.
    fn identity(
        &self,
        selector: &str,
        config_id: &str,
        hub: Option<HubModelConfig>,
    ) -> Result<(String, Option<HubModelConfig>)> {
        if !config_id.is_empty() {
            return Ok((config_id.to_owned(), hub));
        }
        let resolved = self.store.resolve_model(selector)?;
        if !std::path::Path::new(&resolved.key).is_absolute() {
            return Ok((resolved.key, hub));
        }
        let listed = self
            .local_models()?
            .into_iter()
            .find(|model| model.selector == selector && !model.repo_id.is_empty());
        let Some(model) = listed else {
            return Ok((folder_key(&resolved.path), hub));
        };
        let id = model_key(&model.repo_id)?;
        let hub = hub.or(Some(HubModelConfig {
            repo_id: model.repo_id,
            revision: model.revision,
            commit: model.commit,
        }));
        Ok((id, hub))
    }

    /// Whole K/V blocks of the server's block size. The cache is shared by
    /// the requests a model serves at once, so it is useful up to the model's
    /// context for each of them: the most automatic sizing would give it.
    fn context_limits(&self, descriptor: &ModelDescriptor) -> ContextLimits {
        let config = self.library.config();
        let block_size = u64::try_from(config.kv_cache.block_size).unwrap_or(1);
        let requests = u64::try_from(config.scheduler.max_batch_requests).unwrap_or(1).max(1);
        let model_context = u64::try_from(descriptor.metadata().context_len).unwrap_or(u64::MAX);
        ContextLimits {
            block_tokens: std::num::NonZeroU64::new(block_size)
                .unwrap_or(std::num::NonZeroU64::MIN),
            max_tokens: model_context.saturating_mul(requests),
        }
    }
}

const fn options(kv_blocks: Option<std::num::NonZeroU32>) -> ModelLoadOptions {
    ModelLoadOptions {
        allow_memory_overcommit: false,
        kv_blocks,
    }
}

fn resident_tokens(model: &Model) -> u64 {
    let cache = model.kv_cache_config();
    u64::from(cache.block_count).saturating_mul(u64::try_from(cache.block_size).unwrap_or(0))
}

const fn overrides(config: GenerationConfig) -> GenerationOverrides {
    GenerationOverrides {
        max_tokens: config.max_tokens,
        min_tokens: None,
        ignore_eos: None,
        temperature: config.temperature,
        top_p: config.top_p,
        top_k: config.top_k,
        repetition_penalty: config.repetition_penalty,
    }
}
