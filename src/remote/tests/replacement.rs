//! Moving a running listener to another address or certificate never leaves
//! remote access worse off: the running listener stays until its replacement
//! runs, and on the same port the previous one starts again when the new one
//! cannot.

use super::*;

/// A loopback address with a port nothing listens on right now.
fn free_address() -> crate::error::Result<String> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.to_string())
}

/// Whether an existing client still gets answers.
async fn answers(client: &mut rpc::Client) -> bool {
    client.telemetry(proto::TelemetryRequest {}).await.is_ok()
}

#[tokio::test]
async fn a_new_address_that_cannot_bind_keeps_the_running_listener() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let first = remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let listening = first.listening().cloned().expect("listener running");
    let (_, key) = remote.create_key("steady", Scope::Use)?;
    let address = listening.address.to_string();
    let mut client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;

    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    let taken = occupied.local_addr()?.to_string();
    let refused = remote.enable(Some(&taken), service.clone()).await.expect_err("occupied");
    assert!(refused.to_string().contains(&taken), "{refused}");
    let status = remote.status().await?;
    assert_eq!(status.bind(), Some("127.0.0.1:0"), "the setting is restored");
    assert_eq!(status.listening().map(|now| now.address), Some(listening.address));
    assert!(answers(&mut client).await, "the open connection survives");

    let moved = free_address()?;
    let next = remote.enable(Some(&moved), service.clone()).await?;
    let next = next.listening().cloned().expect("the new listener runs");
    assert_eq!(next.address.to_string(), moved);
    assert!(!answers(&mut client).await, "the old listener is gone once replaced");
    let mut fresh = rpc::connect_remote(&target(&moved, &key, next.fingerprint)).await?;
    assert!(answers(&mut fresh).await);
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn enabling_the_running_address_again_keeps_its_connections() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let address = free_address()?;
    let first = remote.enable(Some(&address), service.clone()).await?;
    let listening = first.listening().cloned().expect("listener running");
    let (_, key) = remote.create_key("steady", Scope::Use)?;
    let mut client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;
    let again = remote.enable(Some(&address), service.clone()).await?;
    assert_eq!(again.listening().map(|now| now.fingerprint), Some(listening.fingerprint));
    assert!(answers(&mut client).await, "nothing was restarted");
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn a_certificate_that_cannot_serve_restores_the_previous_listener() -> crate::error::Result<()>
{
    let (service, root) = service();
    let remote = service.remote().clone();
    let address = free_address()?;
    let first = remote.enable(Some(&address), service.clone()).await?;
    let listening = first.listening().cloned().expect("listener running");
    let (_, key) = remote.create_key("steady", Scope::Use)?;

    // A readable certificate with an unusable key: the identity loads, the
    // new listener binds the same port and then fails to configure TLS.
    let other = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])?;
    let certificate = root.join("other-certificate.pem");
    let private_key = root.join("other-key.pem");
    std::fs::write(&certificate, other.cert.pem())?;
    std::fs::write(&private_key, "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n")?;
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let store = Store::new(paths);
    store.set_config_value("server.remote_tls_cert", &certificate.display().to_string())?;
    store.set_config_value("server.remote_tls_key", &private_key.display().to_string())?;

    assert!(remote.enable(Some(&address), service.clone()).await.is_err());
    let status = remote.status().await?;
    let restored = status.listening().cloned().expect("the previous listener runs again");
    assert_eq!(
        (restored.address, restored.fingerprint),
        (listening.address, listening.fingerprint)
    );
    assert_eq!(store.load()?.server.remote_bind.as_deref(), Some(address.as_str()));
    let mut client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;
    assert!(answers(&mut client).await, "the restored listener answers");
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}
