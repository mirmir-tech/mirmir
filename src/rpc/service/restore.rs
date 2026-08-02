use tonic::Status;

use super::{RuntimeService, models::log_progress};

impl RuntimeService {
    pub fn restore_active_models(&self) -> Result<(), Status> {
        let selectors = match self.store.active_models() {
            Ok(selectors) => selectors,
            Err(error) => {
                self.startup.failed(error.to_string());
                return Err(Status::internal(error.to_string()));
            },
        };
        let total = selectors.len();
        self.startup.restoring(total);
        self.report_state_recovery().inspect_err(|error| {
            self.startup.failed(error.message().to_owned());
        })?;
        tracing::info!(models = total, "restoring active models");
        let mut restored = 0_usize;
        let mut failed = 0_usize;
        for selector in selectors {
            let operation = self.activity.begin("restore", &selector, None);
            self.startup.loading(&selector, "preparing model", None, None);
            tracing::info!(model = %selector, "restoring active model");
            let progress_operation = operation.clone();
            let startup = self.startup.clone();
            let startup_target = selector.clone();
            let mut progress = |event: libmir::ProgressEvent| {
                log_progress(&selector, &event);
                startup.loading(
                    &startup_target,
                    &event.detail,
                    Some(event.current),
                    Some(event.total),
                );
                progress_operation.progress(
                    "loading",
                    &event.detail,
                    Some(event.current),
                    Some(event.total),
                );
            };
            match self.load(&selector, false, &mut progress) {
                Ok(_) => {
                    operation.finish("completed", "active model restored");
                    restored = restored.saturating_add(1);
                    tracing::info!(model = %selector, "active model restored");
                },
                Err(error) => {
                    operation.finish("failed", error.message());
                    failed = failed.saturating_add(1);
                    tracing::error!(model = %selector, %error, "failed to restore active model");
                },
            }
        }
        let detail = if failed == 0 {
            format!("runtime ready; {restored} active models restored")
        } else {
            format!("runtime ready; {restored} models restored and {failed} failed")
        };
        self.startup.ready(detail);
        tracing::info!(models = total, restored, failed, "active model restoration finished");
        Ok(())
    }

    pub(super) fn report_state_recovery(&self) -> Result<(), Status> {
        let Some(recovery) = super::status::internal(self.store.take_state_recovery())? else {
            return Ok(());
        };
        let operation = self.activity.begin("recovery", "state.toml", None);
        operation.finish(
            "completed",
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use tokio_stream::StreamExt;

    use super::*;
    use crate::{
        config::{AppConfig, Paths, Store},
        error::Result,
    };

    static NEXT_RESTORE: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn corrupt_state_is_recovered_and_reported_as_activity() -> Result<()> {
        let unique = NEXT_RESTORE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("mirmir-restore-corrupt-{}-{unique}", std::process::id()));
        let paths =
            Paths::from_roots(root.join("config"), root.join("state"), &root.join("runtime"));
        paths.ensure_config_dirs()?;
        std::fs::write(&paths.ux_state_file, "broken = [")?;
        let service = RuntimeService::new(&AppConfig::default(), Store::new(paths));

        service.restore_active_models()?;
        let event = service
            .activity
            .watch(true)
            .next()
            .await
            .expect("recovery event should be present")?;
        assert_eq!(event.kind, "recovery");
        assert_eq!(event.state, "completed");
        assert!(event.detail.contains("state.corrupt-"));
        Ok(())
    }
}
