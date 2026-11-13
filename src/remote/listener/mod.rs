mod connection;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use connection::Closer;
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle, time::timeout};
use tokio_stream::StreamExt;
use tonic::transport::{Identity as TonicIdentity, Server, ServerTlsConfig, server::TcpIncoming};

use super::{
    auth::{AuthLayer, Guard},
    tls::Identity,
};
use crate::{
    error::{Error, Result},
    rpc::{RuntimeService, proto::runtime_server::RuntimeServer},
};

/// How long in-flight calls may finish after the listener stops accepting.
const GRACE: Duration = Duration::from_millis(500);
/// How long the serve task may take to wind down after its connections close.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// The running TLS listener of the `Runtime` service. Stopping or dropping it
/// closes every accepted connection and so every active call.
pub struct Listener {
    pub address: SocketAddr,
    pub identity: Identity,
    shutdown: Option<oneshot::Sender<()>>,
    connections: Option<Closer>,
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
        let closer = Closer::new();
        let incoming = TcpIncoming::from(listener).map({
            let open = closer.handle();
            move |accepted| accepted.map(|stream| open.wrap(stream))
        });
        let task = tokio::spawn(router.serve_with_incoming_shutdown(incoming, async {
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
            connections: Some(closer),
            task,
        })
    }

    /// Stops accepting, lets in-flight calls finish for [`GRACE`], then closes
    /// every remaining connection, open streams included, before returning.
    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _sent = shutdown.send(());
        }
        let finished = if let Ok(finished) = timeout(GRACE, &mut self.task).await {
            Some(finished)
        } else {
            tracing::info!("remote gRPC listener closing its open connections");
            drop(self.connections.take());
            timeout(SHUTDOWN_TIMEOUT, &mut self.task).await.ok()
        };
        drop(self.connections.take());
        match finished {
            Some(Ok(Ok(()))) => {},
            Some(Ok(Err(error))) => {
                tracing::warn!(%error, "remote gRPC listener stopped with an error");
            },
            Some(Err(error)) => tracing::warn!(%error, "remote gRPC listener task failed"),
            None => {
                tracing::warn!("remote gRPC listener did not stop in time; aborting it");
                self.task.abort();
            },
        }
        tracing::info!(address = %self.address, "remote gRPC listener stopped");
    }
}

impl Drop for Listener {
    /// A listener dropped without [`Listener::stop`] still closes its
    /// connections and stops serving.
    fn drop(&mut self) {
        drop(self.connections.take());
        self.task.abort();
    }
}
