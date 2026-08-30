use tonic::Status;

use super::RuntimeService;

impl RuntimeService {
    pub(super) fn report_state_recovery(&self) -> Result<(), Status> {
        self.application.report_state_recovery().map_err(super::status::application)
    }

    pub fn restore_active_models(&self) -> Result<(), Status> {
        let report =
            self.application.restore_active_models().map_err(super::status::application)?;
        tracing::info!(
            models = report.total,
            restored = report.restored,
            failed = report.failed,
            "active model restoration finished"
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
        let event = super::super::activity::watch(&service.application, true)
            .next()
            .await
            .expect("recovery event should be present")?;
        assert_eq!(event.kind, "recovery");
        assert_eq!(event.state, "completed");
        assert!(event.detail.contains("state.corrupt-"));
        Ok(())
    }
}
