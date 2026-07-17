use std::fs;

use tokio::{net::UnixListener, sync::oneshot, task::JoinHandle};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use super::{guard::InstanceGuard, telemetry::Sampler};
use crate::{
    config::{AppConfig, Paths, Store},
    error::Result,
    rpc::{RuntimeService, proto::runtime_server::RuntimeServer},
};

pub struct Owner {
    paths: Paths,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<std::result::Result<(), tonic::transport::Error>>,
    sampler: Sampler,
    _guard: InstanceGuard,
}

pub fn start(paths: Paths, config: &AppConfig) -> Result<Owner> {
    start_service(paths.clone(), RuntimeService::new(config, Store::new(paths)))
}

pub(super) fn start_service(paths: Paths, service: RuntimeService) -> Result<Owner> {
    paths.ensure_runtime_dirs()?;
    let guard = InstanceGuard::acquire(&paths.lock_file)?;
    if paths.socket_file.exists() {
        fs::remove_file(&paths.socket_file)?;
    }
    let listener = UnixListener::bind(&paths.socket_file)?;
    set_socket_permissions(&paths.socket_file)?;
    let incoming = UnixListenerStream::new(listener);
    let (shutdown, receiver) = oneshot::channel();
    let sampler = Sampler::start(service.clone());
    let task = tokio::spawn(async move {
        Server::builder()
            .add_service(RuntimeServer::new(service))
            .serve_with_incoming_shutdown(incoming, async {
                drop(receiver.await);
            })
            .await
    });
    Ok(Owner {
        paths,
        shutdown: Some(shutdown),
        task,
        sampler,
        _guard: guard,
    })
}

impl Owner {
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _shutdown_result = shutdown.send(());
        }
        self.task.await??;
        self.sampler.shutdown().await?;
        if self.paths.socket_file.exists() {
            fs::remove_file(&self.paths.socket_file)?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn set_socket_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_socket_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::rpc::{
        PROTOCOL_VERSION, connect,
        proto::{
            CancelOperationRequest, GetConfigurationRequest, HealthRequest,
            ListActiveModelsRequest, ListLocalModelsRequest, ListModelsRequest, LoadModelRequest,
            SetConfigurationValue, SetHfToken, TelemetryHistoryRequest, TelemetryRequest,
            UnloadModelRequest, UpdateConfigurationRequest, WatchActivityRequest,
            update_configuration_request::Operation,
        },
    };

    static NEXT_SERVER: AtomicU64 = AtomicU64::new(0);

    #[tokio::test]
    async fn serves_health_over_unix_socket_and_cleans_up() -> Result<()> {
        let id = NEXT_SERVER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mirmir-server-{}-{id}", std::process::id()));
        let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
        Store::new(paths.clone()).activate_model("missing")?;
        let owner = start(paths.clone(), &AppConfig::default())?;
        let mut client = connect(&paths.socket_file).await?;
        let health = client.health(HealthRequest {}).await?.into_inner();
        assert_eq!(health.protocol_version, PROTOCOL_VERSION);
        let models = client.list_models(ListModelsRequest {}).await?.into_inner();
        assert!(models.models.is_empty());
        let active = client.list_active_models(ListActiveModelsRequest {}).await?.into_inner();
        assert_eq!(active.selectors, ["missing"]);
        let local = client.list_local_models(ListLocalModelsRequest {}).await?.into_inner();
        assert!(local.models.iter().all(|model| !model.managed));
        let telemetry = client.telemetry(TelemetryRequest {}).await?.into_inner();
        assert_eq!(telemetry.loaded_models, 0);
        assert_eq!(telemetry.total_requests, 0);
        assert!(telemetry.host_total_memory_bytes.is_some());
        let history = await_history(&mut client).await?;
        assert!(!history.samples.is_empty());
        assert_eq!(history.sampling_interval_ms, 1_000);
        let configuration =
            client.get_configuration(GetConfigurationRequest {}).await?.into_inner();
        assert!(configuration.values.iter().any(|value| value.key == "server.http_bind"));
        let updated = client
            .update_configuration(UpdateConfigurationRequest {
                operation: Some(Operation::SetValue(SetConfigurationValue {
                    key: "default_model".to_owned(),
                    value: "test-model".to_owned(),
                })),
            })
            .await?
            .into_inner();
        assert!(!updated.restart_required);
        assert!(updated.message.contains("default_model"));
        let secret = client
            .update_configuration(UpdateConfigurationRequest {
                operation: Some(Operation::SetHfToken(SetHfToken {
                    token: "hf_rpc_secret".to_owned(),
                })),
            })
            .await?
            .into_inner();
        let serialized = serde_json::to_string(&secret)?;
        assert!(!serialized.contains("hf_rpc_secret"));
        assert!(
            secret
                .configuration
                .and_then(|value| value.hugging_face_token)
                .is_some_and(|value| value.configured)
        );
        let mut load = client
            .load_model(LoadModelRequest {
                selector: "missing".to_owned(),
                ..Default::default()
            })
            .await?
            .into_inner();
        let resolving = load.message().await?.expect("resolving event");
        assert_eq!(resolving.phase, "resolving");
        assert!(!resolving.operation_id.is_empty());
        let checking = load.message().await?.expect("memory preflight event");
        assert_eq!(checking.phase, "checking_memory");
        assert!(load.message().await.is_err());
        let mut activity = client
            .watch_activity(WatchActivityRequest { include_history: true })
            .await?
            .into_inner();
        let failed = activity.message().await?.expect("activity history");
        assert_eq!(failed.operation_id, resolving.operation_id);
        assert_eq!(failed.state, "failed");
        assert!(!failed.cancellable);
        let cancelled = client
            .cancel_operation(CancelOperationRequest { operation_id: failed.operation_id })
            .await?
            .into_inner();
        assert!(cancelled.found);
        assert!(!cancelled.accepted);
        let unloaded = client
            .unload_model(UnloadModelRequest { selector: "missing".to_owned() })
            .await?
            .into_inner();
        assert!(!unloaded.unloaded);
        let active = client.list_active_models(ListActiveModelsRequest {}).await?.into_inner();
        assert!(active.selectors.is_empty());
        drop(activity);
        drop(client);
        owner.shutdown().await?;
        assert!(!paths.socket_file.exists());
        assert!(paths.telemetry_file.exists());
        Ok(())
    }

    async fn await_history(
        client: &mut crate::rpc::Client,
    ) -> std::result::Result<crate::rpc::proto::TelemetryHistoryResponse, tonic::Status> {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let history = client
                    .telemetry_history(TelemetryHistoryRequest { limit: 60 })
                    .await?
                    .into_inner();
                if !history.samples.is_empty() {
                    return Ok(history);
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("telemetry sampler should produce a sample")
    }
}
