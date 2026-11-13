use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle, time::timeout};
use tonic::transport::{Identity as TonicIdentity, Server, ServerTlsConfig, server::TcpIncoming};

use super::{
    auth::{AuthLayer, Guard},
    tls::Identity,
};
use crate::{
    error::{Error, Result},
    rpc::{RuntimeService, proto::runtime_server::RuntimeServer},
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// The running TLS listener of the `Runtime` service.
pub struct Listener {
    pub address: SocketAddr,
    pub identity: Identity,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<std::result::Result<(), tonic::transport::Error>>,
}

impl Listener {
    pub async fn start(
        bind: SocketAddr,
        identity: Identity,
        guard: Arc<Guard>,
        service: RuntimeService,
    ) -> Result<Self> {
        let listener = TcpListener::bind(bind)
            .await
            .map_err(|error| Error::Remote(format!("cannot listen on {bind}: {error}")))?;
        let address = listener.local_addr()?;
        let tls = ServerTlsConfig::new()
            .identity(TonicIdentity::from_pem(&identity.certificate, &identity.private_key));
        let (shutdown, signal) = oneshot::channel::<()>();
        let router = Server::builder()
            .tls_config(tls)?
            .layer(AuthLayer::new(guard))
            .add_service(RuntimeServer::new(service));
        let task =
            tokio::spawn(router.serve_with_incoming_shutdown(TcpIncoming::from(listener), async {
                drop(signal.await);
            }));
        tracing::info!(
            %address,
            fingerprint = %identity.fingerprint,
            "remote gRPC listener started"
        );
        Ok(Self {
            address,
            identity,
            shutdown: Some(shutdown),
            task,
        })
    }

    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _sent = shutdown.send(());
        }
        match timeout(SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(Ok(Ok(()))) => {},
            Ok(Ok(Err(error))) => {
                tracing::warn!(%error, "remote gRPC listener stopped with an error");
            },
            Ok(Err(error)) => tracing::warn!(%error, "remote gRPC listener task failed"),
            Err(_) => {
                tracing::warn!("remote gRPC shutdown timed out; closing its connections");
                self.task.abort();
            },
        }
        tracing::info!(address = %self.address, "remote gRPC listener stopped");
    }
}
