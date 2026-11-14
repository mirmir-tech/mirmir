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
    /// Serializes every change of remote access, from the configuration
    /// write to the running listener, so the two never disagree.
    transition: Mutex<()>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    listener: Option<Listener>,
    /// Why the configured listener is not running.
    error: Option<String>,
    /// The server is shutting down: no listener starts again.
    closed: bool,
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
                transition: Mutex::default(),
                state: Mutex::default(),
            }),
        }
    }

    /// Starts the listener when the configuration enables remote access. A
    /// failure is reported in the status; the local server keeps running.
    pub async fn start_configured(&self, service: RuntimeService) {
        let _transition = self.inner.transition.lock().await;
        if self.inner.state.lock().await.closed {
            return;
        }
        let started = match self.configured_bind() {
            Ok(None) => Ok(()),
            Ok(Some(bind)) => match parse(&bind) {
                Ok(address) => self.listen(address, service).await,
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        if let Err(error) = started {
            self.fail(error).await;
        }
    }

    /// Persists the listen address and starts or restarts the listener on it.
    /// When the listener cannot start, the previous setting is restored.
    pub async fn enable(&self, bind: Option<&str>, service: RuntimeService) -> Result<Status> {
        {
            let _transition = self.inner.transition.lock().await;
            if self.inner.state.lock().await.closed {
                return Err(Error::Remote("the server is shutting down".to_owned()));
            }
            let configured = self.configured_bind()?;
            let bind = bind
                .map(str::trim)
                .filter(|bind| !bind.is_empty())
                .map(str::to_owned)
                .or_else(|| configured.clone())
                .unwrap_or_else(|| DEFAULT_REMOTE_BIND.to_owned());
            let address = parse(&bind)?;
            self.persist(Some(&bind))?;
            if let Err(error) = self.listen(address, service).await {
                if let Err(restore) = self.persist(configured.as_deref()) {
                    tracing::error!(%restore, "cannot restore the remote access setting");
                }
                return Err(error);
            }
            self.status().await
        }
    }

    /// Persists that remote access is off, then stops the listener and closes
    /// its connections. When the setting cannot be written nothing changes.
    pub async fn disable(&self) -> Result<Status> {
        {
            let _transition = self.inner.transition.lock().await;
            self.persist(None)?;
            let previous = {
                let mut state = self.inner.state.lock().await;
                state.error = None;
                state.listener.take()
            };
            if let Some(listener) = previous {
                listener.stop().await;
            }
            self.status().await
        }
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

    /// Stops the listener for good; later enables are refused.
    pub async fn shutdown(&self) {
        let _transition = self.inner.transition.lock().await;
        let listener = {
            let mut state = self.inner.state.lock().await;
            state.closed = true;
            state.listener.take()
        };
        if let Some(listener) = listener {
            listener.stop().await;
        }
    }

    /// Replaces the running listener. Callers hold the transition lock.
    async fn listen(&self, address: SocketAddr, service: RuntimeService) -> Result<()> {
        let config = self.inner.store.load()?;
        let identity = tls::identity(&self.inner.store.paths().state_dir, &config.server)?;
        let previous = self.inner.state.lock().await.listener.take();
        if let Some(previous) = previous {
            previous.stop().await;
        }
        let started = Listener::start(address, identity, self.inner.guard.clone(), service).await;
        let (listener, error, result) = match started {
            Ok(listener) => (Some(listener), None, Ok(())),
            Err(error) => (None, Some(error.to_string()), Err(error)),
        };
        let mut state = self.inner.state.lock().await;
        state.listener = listener;
        state.error = error;
        drop(state);
        result
    }

    async fn fail(&self, error: Error) {
        tracing::error!(%error, "remote access is configured but not listening");
        self.inner.state.lock().await.error = Some(error.to_string());
    }

    fn persist(&self, bind: Option<&str>) -> Result<()> {
        self.inner.store.set_remote_bind(bind)
    }

    fn configured_bind(&self) -> Result<Option<String>> {
        Ok(self.inner.store.load()?.server.remote_bind)
    }
}

fn parse(bind: &str) -> Result<SocketAddr> {
    bind.parse()
        .map_err(|error| Error::Config(format!("remote address `{bind}` is invalid: {error}")))
}
