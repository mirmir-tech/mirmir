//! The client side of remote access: TLS to the server's listener with either
//! a pinned certificate fingerprint or the system trust store.

mod verifier;

use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use hyper_util::rt::TokioIo;
use rustls::{ClientConfig, RootCertStore, pki_types::ServerName};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use self::verifier::{Capture, Pinned, provider};
use super::Fingerprint;
use crate::error::{Error, RemoteError, Result};

/// A remote server: `host:port`, the API key sent with every call and how its
/// certificate is trusted.
#[derive(Clone)]
pub struct RemoteTarget {
    pub address: String,
    pub key: String,
    pub trust: Trust,
}

impl std::fmt::Debug for RemoteTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteTarget")
            .field("address", &self.address)
            .field("trust", &self.trust)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// Accept exactly this certificate, as first shown by the server.
    Pinned(Fingerprint),
    /// A certificate issued by a CA in the operating system's trust store.
    System,
}

/// How long reaching a remote server may take: TCP, the TLS handshake and
/// the first call. Long-running calls on an established channel keep their
/// own lifecycle.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs one connection attempt to `address`, failing after `limit`.
pub async fn within<T>(
    address: &str,
    limit: Duration,
    attempt: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(limit, attempt).await.map_err(|_| {
        Error::Remote(RemoteError::Failed(format!(
            "{address} did not complete the connection within {limit:?}; check that it serves \
             mirmir remote access (`mirmir remote enable`) on that port"
        )))
    })?
}

impl RemoteTarget {
    pub async fn channel(&self) -> Result<Channel> {
        let config = Arc::new(client_config(self.trust)?);
        let name = server_name(&self.address)?;
        let address = self.address.clone();
        let endpoint = Endpoint::from_shared(format!("http://{address}")).map_err(|error| {
            Error::Remote(RemoteError::Invalid(format!(
                "`{address}` is not a server address: {error}"
            )))
        })?;
        Ok(endpoint
            .connect_timeout(CONNECT_TIMEOUT)
            .connect_with_connector(service_fn(move |_: Uri| {
                let connector = TlsConnector::from(config.clone());
                let address = address.clone();
                let name = name.clone();
                async move {
                    let tcp = TcpStream::connect(&address).await?;
                    tcp.set_nodelay(true)?;
                    Ok::<_, std::io::Error>(TokioIo::new(connector.connect(name, tcp).await?))
                }
            }))
            .await?)
    }
}

/// The fingerprint of the certificate a server presents, without trusting
/// it: what a client shows before pinning on first use.
pub async fn presented_fingerprint(address: &str) -> Result<Fingerprint> {
    presented_fingerprint_within(address, CONNECT_TIMEOUT).await
}

pub async fn presented_fingerprint_within(address: &str, limit: Duration) -> Result<Fingerprint> {
    within(address, limit, capture_fingerprint(address)).await
}

async fn capture_fingerprint(address: &str) -> Result<Fingerprint> {
    let seen = Arc::new(Mutex::new(None));
    let mut config = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Capture::new(seen.clone())))
        .with_no_client_auth();
    config.alpn_protocols = vec![b"h2".to_vec()];
    let tcp = TcpStream::connect(address).await.map_err(|error| {
        Error::Remote(RemoteError::Failed(format!("cannot reach {address}: {error}")))
    })?;
    TlsConnector::from(Arc::new(config)).connect(server_name(address)?, tcp).await?;
    let fingerprint = *seen
        .lock()
        .map_err(|_| Error::Remote(RemoteError::Failed("poisoned lock".to_owned())))?;
    fingerprint.ok_or_else(|| {
        Error::Remote(RemoteError::Failed(format!("{address} presented no certificate")))
    })
}

fn client_config(trust: Trust) -> Result<ClientConfig> {
    let builder =
        ClientConfig::builder_with_provider(provider()).with_safe_default_protocol_versions()?;
    let mut config = match trust {
        Trust::Pinned(fingerprint) => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pinned::new(fingerprint)))
            .with_no_client_auth(),
        Trust::System => {
            let mut roots = RootCertStore::empty();
            let native = rustls_native_certs::load_native_certs();
            let (added, _ignored) = roots.add_parsable_certificates(native.certs);
            if added == 0 {
                return Err(Error::Remote(RemoteError::Failed(
                    "no system CA certificates found; pass --remote-fingerprint".to_owned(),
                )));
            }
            builder.with_root_certificates(roots).with_no_client_auth()
        },
    };
    config.alpn_protocols = vec![b"h2".to_vec()];
    Ok(config)
}

/// The host part of `host:port`, `[v6]:port` or a bare host.
fn server_name(address: &str) -> Result<ServerName<'static>> {
    let host = address.strip_prefix('[').map_or_else(
        || address.rsplit_once(':').map_or(address, |(host, _)| host),
        |rest| rest.split_once(']').map_or(rest, |(host, _)| host),
    );
    ServerName::try_from(host.to_owned()).map_err(|error| {
        Error::Remote(RemoteError::Invalid(format!("`{host}` is not a valid server name: {error}")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_server_names() -> Result<()> {
        assert_eq!(server_name("ziemniak.local:7443")?.to_str(), "ziemniak.local");
        assert_eq!(server_name("10.0.0.4:7443")?.to_str(), "10.0.0.4");
        assert_eq!(server_name("[::1]:7443")?.to_str(), "::1");
        Ok(())
    }

    #[test]
    fn hides_the_key_from_debug_output() {
        let target = RemoteTarget {
            address: "host:7443".to_owned(),
            key: "mmk_secret".to_owned(),
            trust: Trust::System,
        };
        assert!(!format!("{target:?}").contains("mmk_secret"));
    }
}
