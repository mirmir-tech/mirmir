use libmir::ProgressEvent;

use super::super::ModelEntry;
use crate::application::{
    ActivityKind, ActivityOutcome, ActivityProgress, ActivityStage, Application, Operation, Result,
};

pub struct ModelLoadSession {
    operation: Operation,
}

impl Application {
    #[must_use]
    pub fn start_model_load(&self, selector: &str) -> ModelLoadSession {
        let operation = self.activity.begin(ActivityKind::Load, selector, None);
        operation.progress(
            ActivityStage::Resolving,
            "resolving model",
            Some(ActivityProgress::new(0, None)),
        );
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
                session.operation.finish(ActivityOutcome::Completed, "model is ready");
                Ok(entry)
            },
            Err(error) => {
                session.operation.finish(ActivityOutcome::Failed, &error.to_string());
                Err(error)
            },
        }
    }

    pub fn unload_model(&self, selector: &str) -> Result<bool> {
        let operation = self.activity.begin(ActivityKind::Unload, selector, None);
        let result = self.runtime.unload_model(selector);
        operation.finish(
            if result.is_ok() {
                ActivityOutcome::Completed
            } else {
                ActivityOutcome::Failed
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
            Some(ActivityProgress::new(0, None)),
        );
    }

    pub fn reject(&self, detail: &str) {
        self.operation.finish(ActivityOutcome::Failed, detail);
    }

    fn track(&self, progress: &ProgressEvent) {
        self.operation.progress(
            ActivityStage::Runtime(progress.stage()),
            progress.detail(),
            Some(ActivityProgress::new(
                progress.count().current(),
                Some(progress.count().total()),
            )),
        );
    }
}
