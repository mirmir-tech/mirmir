use std::sync::atomic::{AtomicU64, Ordering};

use tonic::Code;

use super::*;
use crate::{
    config::{AppConfig, Paths, Store},
    rpc::{self, RuntimeService, proto},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn service() -> (RuntimeService, std::path::PathBuf) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-remote-{}-{id}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    (RuntimeService::new(&AppConfig::default(), Store::new(paths)), root)
}

fn target(address: &str, key: &str, fingerprint: Fingerprint) -> RemoteTarget {
    RemoteTarget {
        address: address.to_owned(),
        key: key.to_owned(),
        trust: Trust::Pinned(fingerprint),
    }
}

fn code(error: &crate::error::Error) -> Option<Code> {
    match error {
        crate::error::Error::Status(status) => Some(status.code()),
        _ => None,
    }
}

#[tokio::test]
async fn serves_the_runtime_over_tls_with_pinned_certificates_and_scoped_keys()
-> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let status = remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let listening = status.listening.expect("listener running");
    let address = listening.address.to_string();
    assert_eq!(presented_fingerprint(&address).await?, listening.fingerprint);
    let (_, chat) = remote.create_key("chat", Scope::Use)?;
    let (control_record, control) = remote.create_key("admin", Scope::Control)?;

    let mut client = rpc::connect_remote(&target(&address, &chat, listening.fingerprint)).await?;
    client.list_models(proto::ListModelsRequest {}).await?;
    let denied = client
        .unload_model(proto::UnloadModelRequest { selector: "any".to_owned() })
        .await
        .expect_err("a use key cannot unload");
    assert_eq!(denied.code(), Code::PermissionDenied);

    let mut admin = rpc::connect_remote(&target(&address, &control, listening.fingerprint)).await?;
    admin
        .unload_model(proto::UnloadModelRequest { selector: "any".to_owned() })
        .await?;
    let local_only = admin
        .create_remote_key(proto::CreateRemoteKeyRequest {
            name: "escalate".to_owned(),
            scope: proto::RemoteKeyScope::Control.into(),
        })
        .await
        .expect_err("keys are managed only locally");
    assert_eq!(local_only.code(), Code::PermissionDenied);

    let status = remote.status().await?;
    assert_eq!(status.clients.len(), 2);
    assert!(status.keys.iter().all(|key| key.last_used_unix_ms.is_some()));

    remote.revoke_key(&control_record.id)?;
    let revoked = admin.telemetry(proto::TelemetryRequest {}).await.expect_err("revoked");
    assert_eq!(revoked.code(), Code::Unauthenticated);

    let wrong =
        rpc::connect_remote(&target(&address, "mmk_0011223344556677_bad", listening.fingerprint))
            .await
            .expect_err("an unknown key is rejected");
    assert!(wrong.to_string().contains("valid mirmir remote API key"), "{wrong}");
    assert_eq!(code(&wrong), None);

    let other = Fingerprint::of_der(b"another certificate");
    let mismatch = rpc::connect_remote(&target(&address, &chat, other))
        .await
        .expect_err("a different certificate is refused");
    assert!(
        mismatch.to_string().contains("does not match the pinned fingerprint"),
        "{mismatch}"
    );

    let disabled = remote.disable().await?;
    assert!(disabled.listening.is_none() && disabled.bind.is_none());
    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reports_a_listener_that_cannot_bind() -> crate::error::Result<()> {
    let (service, root) = service();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = occupied.local_addr()?.to_string();
    let remote = service.remote().clone();
    assert!(remote.enable(Some(&address), service.clone()).await.is_err());
    let status = remote.status().await?;
    assert!(status.listening.is_none());
    assert!(status.error.is_some_and(|error| error.contains(&address)));
    assert_eq!(status.bind, None, "a failed start is not persisted");
    std::fs::remove_dir_all(root)?;
    Ok(())
}

mod shutdown;
