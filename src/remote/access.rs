use std::{net::SocketAddr, sync::Arc};

use tokio::sync::Mutex;

use super::{
    Client, KeyRecord, Scope,
    auth::Guard,
    clients::Clients,
    keys::KeyStore,
    listener::Listener,
    tls::{self, CertificateSource, Fingerprint},
};
use crate::{
    config::{DEFAULT_REMOTE_BIND, Store},
    error::{Error, Result},
    rpc::RuntimeService,
};

/// Remote access of one server: its keys, connected clients and the TLS
/// listener, which can be started and stopped while the server runs.
#[derive(Clone)]
pub struct RemoteAccess {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    keys: Arc<KeyStore>,
    clients: Arc<Clients>,
    guard: Arc<Guard>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    listener: Option<Listener>,
    /// Why the configured listener is not running.
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KeyStatus {
    pub record: KeyRecord,
    pub last_used_unix_ms: Option<u64>,
}

/// A running listener's address and certificate.
#[derive(Debug, Clone)]
pub struct Listening {
    pub address: SocketAddr,
    pub fingerprint: Fingerprint,
    pub source: CertificateSource,
}

#[derive(Debug, Clone)]
pub struct Status {
    /// The configured listen address; `None` when remote access is off.
    pub bind: Option<String>,
    pub listening: Option<Listening>,
    pub error: Option<String>,
    pub keys: Vec<KeyStatus>,
    pub clients: Vec<Client>,
}

impl RemoteAccess {
    #[must_use]
    pub fn new(store: Store) -> Self {
        let keys =
            Arc::new(KeyStore::new(store.paths().state_dir.join("remote").join("keys.json")));
        let clients = Arc::new(Clients::default());
        let guard = Arc::new(Guard::new(keys.clone(), clients.clone()));
        Self {
            inner: Arc::new(Inner {
                store,
                keys,
                clients,
                guard,
                state: Mutex::default(),
            }),
        }
    }

    /// Starts the listener when the configuration enables remote access. A
    /// failure is reported in the status; the local server keeps running.
    pub async fn start_configured(&self, service: RuntimeService) {
        let started = match self.configured_bind() {
            Ok(None) => Ok(()),
            Ok(Some(bind)) => self.listen(&bind, service).await,
            Err(error) => Err(error),
        };
        if let Err(error) = started {
            self.fail(error).await;
        }
    }

    /// Starts or restarts the listener and persists its address.
    pub async fn enable(&self, bind: Option<&str>, service: RuntimeService) -> Result<Status> {
        let configured = self.configured_bind()?;
        let bind = bind
            .map(str::trim)
            .filter(|bind| !bind.is_empty())
            .map(str::to_owned)
            .or(configured)
            .unwrap_or_else(|| DEFAULT_REMOTE_BIND.to_owned());
        self.listen(&bind, service).await?;
        self.inner.store.set_config_value("server.remote_bind", &bind)?;
        self.status().await
    }

    pub async fn disable(&self) -> Result<Status> {
        let previous = self.inner.state.lock().await.listener.take();
        if let Some(listener) = previous {
            listener.stop().await;
        }
        self.inner.store.set_config_value("server.remote_bind", "off")?;
        self.inner.state.lock().await.error = None;
        self.status().await
    }

    pub async fn status(&self) -> Result<Status> {
        let keys = self
            .inner
            .keys
            .list()?
            .into_iter()
            .map(|record| KeyStatus {
                last_used_unix_ms: self.inner.clients.last_used(&record.id),
                record,
            })
            .collect();
        let state = self.inner.state.lock().await;
        Ok(Status {
            bind: self.configured_bind()?,
            listening: state.listener.as_ref().map(|listener| Listening {
                address: listener.address,
                fingerprint: listener.identity.fingerprint,
                source: listener.identity.source,
            }),
            error: state.error.clone(),
            keys,
            clients: self.inner.clients.recent(),
        })
    }

    pub fn create_key(&self, name: &str, scope: Scope) -> Result<(KeyRecord, String)> {
        let (record, presented) = self.inner.keys.create(name, scope)?;
        tracing::info!(key = %record.name, id = %record.id, scope = scope.as_str(), "remote API key created");
        Ok((record, presented))
    }

    pub fn revoke_key(&self, id: &str) -> Result<bool> {
        let revoked = self.inner.keys.revoke(id)?;
        if revoked {
            self.inner.clients.forget_key(id);
            tracing::info!(id, "remote API key revoked");
        }
        Ok(revoked)
    }

    pub async fn shutdown(&self) {
        let listener = self.inner.state.lock().await.listener.take();
        if let Some(listener) = listener {
            listener.stop().await;
        }
    }

    #[expect(
        clippy::significant_drop_tightening,
        reason = "the state lock serializes stopping the old listener and starting the new one"
    )]
    async fn listen(&self, bind: &str, service: RuntimeService) -> Result<()> {
        let address: SocketAddr = bind.parse().map_err(|error| {
            Error::Config(format!("remote address `{bind}` is invalid: {error}"))
        })?;
        let config = self.inner.store.load()?;
        let identity = tls::identity(&self.inner.store.paths().state_dir, &config.server)?;
        let mut state = self.inner.state.lock().await;
        if let Some(previous) = state.listener.take() {
            previous.stop().await;
        }
        let listener = Listener::start(address, identity, self.inner.guard.clone(), service).await;
        match listener {
            Ok(listener) => {
                state.listener = Some(listener);
                state.error = None;
                Ok(())
            },
            Err(error) => {
                state.error = Some(error.to_string());
                Err(error)
            },
        }
    }

    async fn fail(&self, error: Error) {
        tracing::error!(%error, "remote access is configured but not listening");
        self.inner.state.lock().await.error = Some(error.to_string());
    }

    fn configured_bind(&self) -> Result<Option<String>> {
        Ok(self.inner.store.load()?.server.remote_bind)
    }
}
