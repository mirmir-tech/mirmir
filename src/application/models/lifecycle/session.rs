use libmir::ProgressEvent;

use super::super::ModelEntry;
use crate::application::{Application, Operation, Result};

pub struct ModelLoadSession {
    operation: Operation,
}

impl Application {
    #[must_use]
    pub fn start_model_load(&self, selector: &str) -> ModelLoadSession {
        let operation = self.activity.begin("load", selector, None);
        operation.progress("resolving", "resolving model", Some(0), None);
        ModelLoadSession { operation }
    }

    pub fn load_model(
        &self,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry> {
        let session = self.start_model_load(selector);
        self.load_model_session(&session, selector, force, progress)
    }

    pub fn load_model_session(
        &self,
        session: &ModelLoadSession,
        selector: &str,
        force: bool,
        progress: &mut dyn FnMut(ProgressEvent),
    ) -> Result<ModelEntry> {
        let mut tracked = |event: ProgressEvent| {
            session.track(&event);
            progress(event);
        };
        match self.runtime.load_model(selector, force, &mut tracked) {
            Ok(entry) => {
                session.operation.finish("completed", "model is ready");
                Ok(entry)
            },
            Err(error) => {
                session.operation.finish("failed", &error.to_string());
                Err(error)
            },
        }
    }

    pub fn unload_model(&self, selector: &str) -> Result<bool> {
        let operation = self.activity.begin("unload", selector, None);
        let result = self.runtime.unload_model(selector);
        operation.finish(
            if result.is_ok() {
                "completed"
            } else {
                "failed"
            },
            "model unload finished",
        );
        result
    }
}

impl ModelLoadSession {
    #[must_use]
    pub fn operation_id(&self) -> &str {
        self.operation.id()
    }

    pub fn checking_memory(&self) {
        self.operation.progress(
            "checking_memory",
            "checking weights, KV cache, workspace, and device budget",
            Some(0),
            None,
        );
    }

    pub fn reject(&self, detail: &str) {
        self.operation.finish("failed", detail);
    }

    fn track(&self, progress: &ProgressEvent) {
        let stage = match progress.stage {
            libmir::ProgressStage::LoadWeights
                if progress.total > 0 && progress.current >= progress.total =>
            {
                "initializing"
            },
            libmir::ProgressStage::LoadWeights => "loading",
            libmir::ProgressStage::PrefillTokens => "warming",
            libmir::ProgressStage::DecodeTokens => "decoding",
        };
        self.operation.progress(
            stage,
            &progress.detail,
            Some(progress.current),
            Some(progress.total),
        );
    }
}
