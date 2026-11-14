mod listen;
mod status;

use std::{net::SocketAddr, sync::Arc};

use status::Slot;
pub use status::{KeyStatus, ListenerState, Mode, Status};
use tokio::sync::Mutex;

use super::{KeyRecord, Scope, auth::Guard, clients::Clients, keys::KeyStore};
use crate::{
    config::{DEFAULT_REMOTE_BIND, Store},
    error::{Error, RemoteError, Result},
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
    state: Mutex<Slot>,
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
        if matches!(*self.inner.state.lock().await, Slot::Closed) {
            return;
        }
        let bind = match self.configured_bind() {
            Ok(Some(bind)) => bind,
            Ok(None) => return,
            Err(error) => return self.fail(None, &error).await,
        };
        let started = match parse(&bind) {
            Ok(address) => self.listen(&bind, address, service).await,
            Err(error) => Err(error),
        };
        if let Err(error) = started {
            self.fail(Some(bind), &error).await;
        }
    }

    /// Persists the listen address and starts or restarts the listener on it.
    /// When the listener cannot start, the previous setting and listener stay.
    pub async fn enable(&self, bind: Option<&str>, service: RuntimeService) -> Result<Status> {
        let _transition = self.inner.transition.lock().await;
        if matches!(*self.inner.state.lock().await, Slot::Closed) {
            return Err(Error::Remote(RemoteError::ShuttingDown));
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
        if let Err(error) = self.listen(&bind, address, service).await {
            if let Err(restore) = self.persist(configured.as_deref()) {
                tracing::error!(%restore, "cannot restore the remote access setting");
            }
            return Err(error);
        }
        self.status().await
    }

    /// Persists that remote access is off, then stops the listener and closes
    /// its connections. When the setting cannot be written nothing changes.
    pub async fn disable(&self) -> Result<Status> {
        let _transition = self.inner.transition.lock().await;
        self.persist(None)?;
        let previous = self.inner.state.lock().await.take();
        if let Some(listener) = previous {
            listener.stop().await;
        }
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
        let configured = self.configured_bind()?;
        Ok(Status {
            mode: self.inner.state.lock().await.mode(configured),
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
        let previous = std::mem::replace(&mut *self.inner.state.lock().await, Slot::Closed);
        if let Slot::Running { listener, .. } = previous {
            listener.stop().await;
        }
    }

    /// Records why the configured listener is not running. Without a
    /// readable address there is nothing to report it against beyond the log.
    async fn fail(&self, bind: Option<String>, error: &Error) {
        tracing::error!(%error, "remote access is configured but not listening");
        if let Some(bind) = bind {
            *self.inner.state.lock().await = Slot::Failed { bind, error: error.to_string() };
        }
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
