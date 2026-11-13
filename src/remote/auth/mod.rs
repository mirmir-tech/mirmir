//! Authentication and authorization of every request on the TLS listener.
//! The local Unix socket never passes through this layer.

mod failures;
#[cfg(test)]
mod tests;

use std::{
    future::{Future, ready},
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use tonic::{
    Status,
    codegen::http::{HeaderMap, Request, Response},
    transport::server::{TcpConnectInfo, TlsConnectInfo},
};
use tower::{Layer, Service};

use self::failures::Failures;
use super::{
    Access, KeyRecord,
    clients::Clients,
    keys::KeyStore,
    scope::{method, required},
};

pub struct Guard {
    keys: Arc<KeyStore>,
    clients: Arc<Clients>,
    failures: Failures,
}

impl Guard {
    #[must_use]
    pub fn new(keys: Arc<KeyStore>, clients: Arc<Clients>) -> Self {
        Self {
            keys,
            clients,
            failures: Failures::default(),
        }
    }

    /// Unauthenticated without a valid key, permission denied for a scope
    /// that is too narrow or a local-only method.
    pub fn authorize(
        &self,
        path: &str,
        headers: &HeaderMap,
        peer: Option<SocketAddr>,
    ) -> Result<KeyRecord, Status> {
        let host = peer.map(|peer| peer.ip());
        if self.failures.blocked(host) {
            return Err(Status::resource_exhausted(
                "too many failed authentication attempts; retry in a minute",
            ));
        }
        let name = method(path);
        let presented = bearer(headers);
        let key = match presented.map(|key| self.keys.verify(key)).transpose() {
            Ok(Some(Some(key))) => key,
            Ok(_) => {
                self.failures.record(host);
                tracing::warn!(peer = ?peer, method = name.unwrap_or(path), key_present = presented.is_some(), "rejected a remote request without a valid API key");
                return Err(Status::unauthenticated("a valid mirmir remote API key is required"));
            },
            Err(error) => {
                tracing::error!(%error, "cannot read the remote API keys");
                return Err(Status::internal("the server cannot read its remote API keys"));
            },
        };
        let Some(name) = name else {
            return Err(Status::unimplemented("unknown service"));
        };
        match required(name) {
            None => Err(Status::unimplemented(format!("unknown method {name}"))),
            Some(Access::LocalOnly) => Err(Status::permission_denied(format!(
                "{name} is available only over the server's local socket"
            ))),
            Some(Access::Remote(scope)) if !key.scope.allows(scope) => {
                Err(Status::permission_denied(format!(
                    "key `{}` has the {} scope; {name} requires {}",
                    key.name,
                    key.scope.as_str(),
                    scope.as_str()
                )))
            },
            Some(Access::Remote(_)) => {
                self.clients.record(&key, peer);
                Ok(key)
            },
        }
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

fn peer<B>(request: &Request<B>) -> Option<SocketAddr> {
    let extensions = request.extensions();
    extensions
        .get::<TlsConnectInfo<TcpConnectInfo>>()
        .and_then(|info| info.get_ref().remote_addr())
        .or_else(|| extensions.get::<TcpConnectInfo>().and_then(TcpConnectInfo::remote_addr))
}

#[derive(Clone)]
pub struct AuthLayer {
    guard: Arc<Guard>,
}

impl AuthLayer {
    #[must_use]
    pub const fn new(guard: Arc<Guard>) -> Self {
        Self { guard }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = Authenticated<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Authenticated { inner, guard: self.guard.clone() }
    }
}

#[derive(Clone)]
pub struct Authenticated<S> {
    inner: S,
    guard: Arc<Guard>,
}

type Pending<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

impl<S, B> Service<Request<B>> for Authenticated<S>
where
    S: Service<Request<B>, Response = Response<tonic::body::Body>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Error = S::Error;
    type Future = Pending<Self::Response, Self::Error>;
    type Response = Response<tonic::body::Body>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        match self.guard.authorize(request.uri().path(), request.headers(), peer(&request)) {
            Ok(_) => Box::pin(self.inner.call(request)),
            Err(status) => Box::pin(ready(Ok(status.into_http()))),
        }
    }
}
