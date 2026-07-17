use std::time::Duration;

use crate::{
    config::{AppConfig, Paths},
    error::{Error, Result},
    rpc,
};

const CONNECT_ATTEMPTS: usize = 80;
const CONNECT_RETRY_DELAY: Duration = Duration::from_millis(25);

pub struct Connection {
    pub client: rpc::Client,
    owner: Option<super::server::Owner>,
}

impl Connection {
    #[must_use]
    pub const fn reused(&self) -> bool {
        self.owner.is_none()
    }

    pub async fn shutdown(self) -> Result<()> {
        if let Some(owner) = self.owner {
            owner.shutdown().await
        } else {
            Ok(())
        }
    }
}

pub async fn connect_or_start(paths: &Paths, config: &AppConfig) -> Result<Connection> {
    match rpc::connect(&paths.socket_file).await {
        Ok(client) => return Ok(Connection { client, owner: None }),
        Err(error) if retryable(&error) => {},
        Err(error) => return Err(error),
    }
    retry_connect(paths, config, try_start(paths, config)?).await
}

fn try_start(paths: &Paths, config: &AppConfig) -> Result<Option<super::server::Owner>> {
    match super::server::start(paths.clone(), config) {
        Ok(owner) => Ok(Some(owner)),
        Err(Error::AlreadyRunning(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn retry_connect(
    paths: &Paths,
    config: &AppConfig,
    mut owner: Option<super::server::Owner>,
) -> Result<Connection> {
    let mut last_error = None;
    for attempt in 0..CONNECT_ATTEMPTS {
        match rpc::connect(&paths.socket_file).await {
            Ok(client) => return Ok(Connection { client, owner }),
            Err(error) if retryable(&error) => last_error = Some(error.to_string()),
            Err(error) => return cleanup(owner, error).await,
        }
        if owner.is_none() {
            owner = try_start(paths, config)?;
        }
        if attempt + 1 < CONNECT_ATTEMPTS {
            tokio::time::sleep(CONNECT_RETRY_DELAY).await;
        }
    }
    let detail = last_error.unwrap_or_else(|| "unknown transport error".to_owned());
    let error = Error::Config(format!(
        "mirmir server at {} did not become reachable within 2 seconds: {detail}",
        paths.socket_file.display()
    ));
    cleanup(owner, error).await
}

fn retryable(error: &Error) -> bool {
    match error {
        Error::Transport(_) => true,
        Error::Status(status) => status.code() == tonic::Code::Unavailable,
        _ => false,
    }
}

async fn cleanup(owner: Option<super::server::Owner>, error: Error) -> Result<Connection> {
    if let Some(owner) = owner
        && let Err(cleanup) = owner.shutdown().await
    {
        tracing::warn!(%cleanup, "failed to clean up server after connection failure");
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::rpc::proto::HealthRequest;

    static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn recovers_a_stale_socket_file() -> Result<()> {
        let paths = paths("stale");
        paths.ensure_runtime_dirs()?;
        std::fs::write(&paths.socket_file, "stale")?;
        let mut connection = connect_or_start(&paths, &AppConfig::default()).await?;
        assert!(!connection.reused());
        connection.client.health(HealthRequest {}).await?;
        connection.shutdown().await?;
        assert!(!paths.socket_file.exists());
        Ok(())
    }

    #[tokio::test]
    async fn simultaneous_clients_share_one_server() -> Result<()> {
        let paths = paths("race");
        let config = AppConfig::default();
        let (left, right) =
            tokio::join!(connect_or_start(&paths, &config), connect_or_start(&paths, &config));
        let mut left = left?;
        let mut right = right?;
        assert_ne!(left.reused(), right.reused());
        left.client.health(HealthRequest {}).await?;
        right.client.health(HealthRequest {}).await?;
        if left.reused() {
            left.shutdown().await?;
            right.shutdown().await?;
        } else {
            right.shutdown().await?;
            left.shutdown().await?;
        }
        assert!(!paths.socket_file.exists());
        Ok(())
    }

    #[tokio::test]
    async fn claims_startup_after_a_competing_owner_disappears() -> Result<()> {
        let paths = paths("abandoned");
        paths.ensure_runtime_dirs()?;
        let guard = super::super::guard::InstanceGuard::acquire(&paths.lock_file)?;
        let pending_paths = paths.clone();
        let task =
            tokio::spawn(
                async move { connect_or_start(&pending_paths, &AppConfig::default()).await },
            );
        tokio::time::sleep(Duration::from_millis(75)).await;
        drop(guard);
        let mut connection = task.await??;
        assert!(!connection.reused());
        connection.client.health(HealthRequest {}).await?;
        connection.shutdown().await?;
        Ok(())
    }

    fn paths(label: &str) -> Paths {
        let id = NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("mirmir-connection-{label}-{}-{id}", std::process::id()));
        Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"))
    }
}
