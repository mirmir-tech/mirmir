//! Starting the listener on a new address or certificate without losing the
//! running one when the new one cannot start.

use std::net::SocketAddr;

use super::{RemoteAccess, Slot};
use crate::{
    error::Result,
    remote::{
        listener::Listener,
        tls::{self, Identity},
    },
    rpc::RuntimeService,
};

impl RemoteAccess {
    /// Runs the listener for `bind` on `address`. Callers hold the transition
    /// lock. On failure remote access stays as it was: the previous listener
    /// keeps running, or is started again, and the error is returned.
    pub(super) async fn listen(
        &self,
        bind: &str,
        address: SocketAddr,
        service: RuntimeService,
    ) -> Result<()> {
        let config = self.inner.store.load()?;
        let identity = tls::identity(&self.inner.store.paths().state_dir, &config.server)?;
        let running = match &*self.inner.state.lock().await {
            Slot::Running { listener, .. } => {
                Some((listener.address, listener.identity.fingerprint))
            },
            Slot::Idle | Slot::Failed { .. } | Slot::Closed => None,
        };
        match running {
            // Same socket and certificate: the running listener already is
            // the requested one, so its connections stay open.
            Some((current, fingerprint))
                if current == address && fingerprint == identity.fingerprint =>
            {
                if let Slot::Running { bind: running, .. } = &mut *self.inner.state.lock().await {
                    bind.clone_into(running);
                }
                Ok(())
            },
            // The port is taken by the running listener, which must stop
            // before its replacement can bind.
            Some((current, _)) if address.port() != 0 && current.port() == address.port() => {
                self.restart(bind, address, identity, service).await
            },
            _ => self.replace(bind, address, identity, service).await,
        }
    }

    /// Binds the new listener first and stops the previous one only once the
    /// new one runs.
    async fn replace(
        &self,
        bind: &str,
        address: SocketAddr,
        identity: Identity,
        service: RuntimeService,
    ) -> Result<()> {
        let listener =
            Listener::start(address, identity, self.inner.guard.clone(), service).await?;
        let previous = std::mem::replace(
            &mut *self.inner.state.lock().await,
            Slot::Running { bind: bind.to_owned(), listener },
        );
        if let Slot::Running { listener, .. } = previous {
            listener.stop().await;
        }
        Ok(())
    }

    /// Stops the running listener, starts the new one on the same port, and
    /// starts the previous one again when the new one cannot start.
    async fn restart(
        &self,
        bind: &str,
        address: SocketAddr,
        identity: Identity,
        service: RuntimeService,
    ) -> Result<()> {
        let previous = std::mem::take(&mut *self.inner.state.lock().await);
        let Slot::Running { bind: previous_bind, listener: previous } = previous else {
            *self.inner.state.lock().await = previous;
            return self.replace(bind, address, identity, service).await;
        };
        let (previous_address, previous_identity) = (previous.address, previous.identity.clone());
        previous.stop().await;
        let guard = self.inner.guard.clone();
        let error = match Listener::start(address, identity, guard.clone(), service.clone()).await {
            Ok(listener) => {
                *self.inner.state.lock().await = Slot::Running { bind: bind.to_owned(), listener };
                return Ok(());
            },
            Err(error) => error,
        };
        let restored = Listener::start(previous_address, previous_identity, guard, service).await;
        *self.inner.state.lock().await = match restored {
            Ok(listener) => Slot::Running { bind: previous_bind, listener },
            Err(restore) => {
                tracing::error!(%restore, "cannot restart the previous remote listener");
                Slot::Failed {
                    bind: previous_bind,
                    error: restore.to_string(),
                }
            },
        };
        Err(error)
    }
}
