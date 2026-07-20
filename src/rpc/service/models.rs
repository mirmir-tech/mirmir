use libmir::{GenerationOverrides, Model, ProgressEvent, ProgressStage, ProgressUnit};
use tokio::sync::mpsc;
use tonic::Status;

use super::RuntimeService;
use crate::rpc::proto;

mod capability;
mod events;

pub(super) use self::events::log_progress;
use self::{
    capability::model_info,
    events::{LifecycleState, checking_memory, event},
};

pub struct ModelEntry {
    pub model: Model,
    pub info: proto::ModelInfo,
}

impl RuntimeService {
    pub(super) fn load(
        &self,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry, Status> {
        let resolved = self
            .store
            .resolve_model(selector)
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        if let Some(model) = self
            .models
            .lock()
            .map_err(|_| Status::internal("model registry lock is poisoned"))?
            .get(&resolved.key)
        {
            return Ok(model.clone());
        }
        let defaults = GenerationOverrides {
            max_tokens: resolved.generation.max_tokens,
            temperature: resolved.generation.temperature,
            top_p: resolved.generation.top_p,
            top_k: resolved.generation.top_k,
            repetition_penalty: resolved.generation.repetition_penalty,
        };
        super::preflight::check(&self.library, &resolved.path, defaults, &resolved.key, force)?;
        let mut loading = self
            .loading
            .lock()
            .map_err(|_| Status::internal("model lifecycle lock is poisoned"))?;
        if !loading.insert(resolved.key.clone()) {
            return Err(Status::failed_precondition("model is already loading"));
        }
        drop(loading);
        let path = resolved.path.display().to_string();
        let loaded = self.library.load(resolved.path, defaults, progress);
        let mut loading = self
            .loading
            .lock()
            .map_err(|_| Status::internal("model lifecycle lock is poisoned"))?;
        loading.remove(&resolved.key);
        drop(loading);
        let loaded = loaded.map_err(|error| Status::internal(error.to_string()))?;
        let entry = ModelEntry {
            info: model_info(resolved.key.clone(), path, &loaded),
            model: loaded,
        };
        let mut models = self
            .models
            .lock()
            .map_err(|_| Status::internal("model registry lock is poisoned"))?;
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

    pub(super) fn list(&self) -> Result<Vec<proto::ModelInfo>, Status> {
        let models = self
            .models
            .lock()
            .map_err(|_| Status::internal("model registry lock is poisoned"))?;
        let mut listed = models.values().map(|entry| entry.info.clone()).collect::<Vec<_>>();
        drop(models);
        listed.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(listed)
    }

    pub(super) fn unload(&self, selector: &str) -> Result<bool, Status> {
        let key = self
            .store
            .resolve_model(selector)
            .map_or_else(|_| selector.to_owned(), |model| model.key);
        let mut models = self
            .models
            .lock()
            .map_err(|_| Status::internal("model registry lock is poisoned"))?;
        let entry = models.remove(selector).or_else(|| models.remove(&key));
        let Some(entry) = entry else {
            drop(models);
            self.store
                .deactivate_model(&key)
                .map_err(|error| Status::internal(error.to_string()))?;
            tracing::info!(model = %key, unloaded = false, "model unload requested");
            return Ok(false);
        };
        if entry.model.is_in_use() {
            models.insert(entry.info.id.clone(), entry);
            return Err(Status::failed_precondition("model is currently serving a request"));
        }
        drop(models);
        entry.model.unload().map_err(|error| Status::internal(error.to_string()))?;
        self.store
            .deactivate_model(&key)
            .map_err(|error| Status::internal(error.to_string()))?;
        tracing::info!(model = %key, unloaded = true, "model unload requested");
        Ok(true)
    }
}

pub fn stream_load(
    service: &RuntimeService,
    request: &proto::LoadModelRequest,
    sender: &mpsc::Sender<Result<proto::ModelLifecycleEvent, Status>>,
) {
    let requested = request.selector.clone();
    let operation = service.activity.begin("load", &requested, None);
    operation.progress("resolving", "resolving model", Some(0), None);
    tracing::info!(model = %requested, "model load requested");
    send(
        sender,
        event(
            operation.id(),
            &requested,
            "resolving",
            LifecycleState {
                current: 0,
                total: None,
                unit: "item",
                detail: "resolving model",
                model: None,
            },
        ),
    );
    let selector = match service.prepare_load(request) {
        Ok(selector) => selector,
        Err(error) => {
            operation.finish("failed", error.message());
            drop(sender.blocking_send(Err(error)));
            return;
        },
    };
    operation.progress(
        "checking_memory",
        "checking weights, KV cache, workspace, and device budget",
        Some(0),
        None,
    );
    send(sender, checking_memory(operation.id(), &selector));
    let mut progress = |progress: ProgressEvent| {
        log_progress(&selector, &progress);
        let phase = match progress.stage {
            ProgressStage::LoadWeights
                if progress.total > 0 && progress.current >= progress.total =>
            {
                "initializing"
            },
            ProgressStage::LoadWeights => "loading",
            ProgressStage::PrefillTokens => "warming",
            ProgressStage::DecodeTokens => "decoding",
        };
        let unit = match progress.unit {
            ProgressUnit::Byte => "byte",
            ProgressUnit::Token => "token",
        };
        operation.progress(phase, &progress.detail, Some(progress.current), Some(progress.total));
        send(
            sender,
            event(
                operation.id(),
                &selector,
                phase,
                LifecycleState {
                    current: progress.current,
                    total: Some(progress.total),
                    unit,
                    detail: &progress.detail,
                    model: None,
                },
            ),
        );
    };
    match service.load(&selector, request.force, &mut progress) {
        Ok(entry) => {
            operation.finish("completed", "model is ready");
            tracing::info!(model = %entry.info.id, path = %entry.info.path, "model is ready");
            send(
                sender,
                event(
                    operation.id(),
                    &requested,
                    "ready",
                    LifecycleState {
                        current: 1,
                        total: Some(1),
                        unit: "item",
                        detail: "model is ready",
                        model: Some(entry.info),
                    },
                ),
            );
        },
        Err(error) => {
            operation.finish("failed", error.message());
            tracing::error!(model = %selector, %error, "model load failed");
            drop(sender.blocking_send(Err(error)));
        },
    }
}

fn send(
    sender: &mpsc::Sender<Result<proto::ModelLifecycleEvent, Status>>,
    event: proto::ModelLifecycleEvent,
) {
    drop(sender.blocking_send(Ok(event)));
}

impl Clone for ModelEntry {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            info: self.info.clone(),
        }
    }
}
