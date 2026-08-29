use libmir::ProgressEvent;

use super::super::ModelEntry;
use crate::application::{
    ActivityKind, ActivityStage, ActivityState, Application, Operation, Result,
};

pub struct ModelLoadSession {
    operation: Operation,
}

impl Application {
    #[must_use]
    pub fn start_model_load(&self, selector: &str) -> ModelLoadSession {
        let operation = self.activity.begin(ActivityKind::Load, selector, None);
        operation.progress(ActivityStage::Resolving, "resolving model", Some(0), None);
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
                session.operation.finish(ActivityState::Completed, "model is ready");
                Ok(entry)
            },
            Err(error) => {
                session.operation.finish(ActivityState::Failed, &error.to_string());
                Err(error)
            },
        }
    }

    pub fn unload_model(&self, selector: &str) -> Result<bool> {
        let operation = self.activity.begin(ActivityKind::Unload, selector, None);
        let result = self.runtime.unload_model(selector);
        operation.finish(
            if result.is_ok() {
                ActivityState::Completed
            } else {
                ActivityState::Failed
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
            ActivityStage::CheckingMemory,
            "checking weights, KV cache, workspace, and device budget",
            Some(0),
            None,
        );
    }

    pub fn reject(&self, detail: &str) {
        self.operation.finish(ActivityState::Failed, detail);
    }

    fn track(&self, progress: &ProgressEvent) {
        self.operation.progress(
            ActivityStage::Runtime(progress.stage),
            &progress.detail,
            Some(progress.current),
            Some(progress.total),
        );
    }
}
