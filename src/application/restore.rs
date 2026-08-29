use super::{ActivityKind, ActivityOutcome, ActivityProgress, ActivityStage, Application, Result};

pub struct RestoreReport {
    pub total: usize,
    pub restored: usize,
    pub failed: usize,
}

impl Application {
    pub fn restore_active_models(&self) -> Result<RestoreReport> {
        let selectors = self.runtime.active_models().inspect_err(|error| {
            self.startup.failed(error.to_string());
        })?;
        let total = selectors.len();
        self.startup.restoring(total);
        self.report_state_recovery()?;
        let mut report = RestoreReport { total, restored: 0, failed: 0 };
        for selector in selectors {
            self.restore_model(&selector, &mut report);
        }
        let detail = if report.failed == 0 {
            format!("runtime ready; {} active models restored", report.restored)
        } else {
            format!(
                "runtime ready; {} models restored and {} failed",
                report.restored, report.failed
            )
        };
        self.startup.ready(detail);
        Ok(report)
    }

    fn restore_model(&self, selector: &str, report: &mut RestoreReport) {
        let operation = self.activity.begin(ActivityKind::Restore, selector, None);
        self.startup.loading(selector, "preparing model", None);
        let startup = self.startup.clone();
        let operation_progress = operation.clone();
        let mut progress = |event: libmir::ProgressEvent| {
            let count = event.count();
            startup.loading(selector, event.detail(), Some(count));
            operation_progress.progress(
                ActivityStage::Runtime(event.stage()),
                event.detail(),
                Some(ActivityProgress::new(count.current(), Some(count.total()))),
            );
        };
        match self.runtime.load_model(selector, false, &mut progress) {
            Ok(_) => {
                operation.finish(ActivityOutcome::Completed, "active model restored");
                report.restored = report.restored.saturating_add(1);
            },
            Err(error) => {
                operation.finish(ActivityOutcome::Failed, &error.to_string());
                report.failed = report.failed.saturating_add(1);
                tracing::error!(model = %selector, %error, "failed to restore active model");
            },
        }
    }

    pub fn report_state_recovery(&self) -> Result<()> {
        let Some(recovery) = self.runtime.take_state_recovery()? else {
            return Ok(());
        };
        let operation = self.activity.begin(ActivityKind::Recovery, "state.toml", None);
        operation.finish(
            ActivityOutcome::Completed,
            &format!(
                "corrupt {} quarantined at {} ({})",
                recovery.original.display(),
                recovery.quarantine.display(),
                recovery.reason
            ),
        );
        Ok(())
    }
}
