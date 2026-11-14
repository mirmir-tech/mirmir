//! Reaching a remote server is bounded: a peer that accepts TCP but never
//! completes TLS, or completes TLS but never answers Health, fails the
//! command with a timeout instead of holding it forever.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio::{io::AsyncReadExt, net::TcpListener};
use tokio_rustls::TlsAcceptor;

use super::*;
use crate::remote::presented_fingerprint_within;

const LIMIT: Duration = Duration::from_millis(300);

/// Accepts connections and keeps them open, reading and ignoring what
/// arrives; with `tls`, it completes the handshake first.
async fn silent_peer(tls: Option<TlsAcceptor>) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let tls = tls.clone();
            tokio::spawn(async move {
                let mut sink = [0_u8; 4096];
                match tls {
                    None => {
                        let mut tcp = tcp;
                        while tcp.read(&mut sink).await.is_ok_and(|read| read > 0) {}
                    },
                    Some(acceptor) => {
                        if let Ok(mut stream) = acceptor.accept(tcp).await {
                            while stream.read(&mut sink).await.is_ok_and(|read| read > 0) {}
                        }
                    },
                }
            });
        }
    });
    Ok(address)
}

fn assert_timed_out(result: crate::error::Result<impl std::fmt::Debug>, started: Instant) {
    let error = result.expect_err("a silent peer fails the connection");
    assert!(error.to_string().contains("did not complete the connection"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(3), "took {:?}", started.elapsed());
}

#[tokio::test]
async fn a_peer_that_never_completes_tls_times_out() -> crate::error::Result<()> {
    let address = silent_peer(None).await?;
    let started = Instant::now();
    assert_timed_out(presented_fingerprint_within(&address, LIMIT).await, started);
    let pinned = Fingerprint::of_der(b"never presented");
    let started = Instant::now();
    let connected = rpc::connect_remote_within(&target(&address, "mmk_key", pinned), LIMIT).await;
    assert_timed_out(connected.map(|_| ()), started);
    Ok(())
}

#[tokio::test]
async fn a_peer_that_never_answers_health_times_out() -> crate::error::Result<()> {
    let (_, root) = service();
    let identity = crate::remote::tls::identity(
        &root.join("state"),
        &crate::config::AppConfig::default().server,
    )?;
    let certificate =
        CertificateDer::from_pem_slice(identity.certificate.as_bytes()).map_err(|error| {
            crate::error::Error::Remote(crate::error::RemoteError::Failed(error.to_string()))
        })?;
    let key = PrivateKeyDer::from_pem_slice(identity.private_key.as_bytes()).map_err(|error| {
        crate::error::Error::Remote(crate::error::RemoteError::Failed(error.to_string()))
    })?;
    let mut config = ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(vec![certificate], key)?;
    config.alpn_protocols = vec![b"h2".to_vec()];
    let address = silent_peer(Some(TlsAcceptor::from(Arc::new(config)))).await?;

    assert_eq!(presented_fingerprint_within(&address, LIMIT).await?, identity.fingerprint);
    let started = Instant::now();
    let connected =
        rpc::connect_remote_within(&target(&address, "mmk_key", identity.fingerprint), LIMIT).await;
    assert_timed_out(connected.map(|_| ()), started);
    drop(std::fs::remove_dir_all(root));
    Ok(())
}
