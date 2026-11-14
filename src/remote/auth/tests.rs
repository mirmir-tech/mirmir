use std::{fs, path::PathBuf};

use tonic::{Code, codegen::http::HeaderValue};

use super::*;
use crate::remote::Scope;

struct Fixture {
    guard: Guard,
    keys: Arc<KeyStore>,
    clients: Arc<Clients>,
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("mirmir-auth-{name}-{}", std::process::id()));
        drop(fs::remove_dir_all(&root));
        let keys = Arc::new(KeyStore::new(root.join("keys.json")));
        let clients = Arc::new(Clients::default());
        Self {
            guard: Guard::new(keys.clone(), clients.clone()),
            keys,
            clients,
            root,
        }
    }

    fn call(&self, method: &str, key: Option<&str>) -> Result<KeyRecord, Code> {
        let mut headers = HeaderMap::new();
        if let Some(key) = key {
            headers.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {key}")).expect("header value"),
            );
        }
        let peer = Some(SocketAddr::from(([192, 168, 1, 20], 51_000)));
        self.guard
            .authorize(&format!("/mirmir.v1.Runtime/{method}"), bearer(&headers), peer)
            .map_err(|status| status.code())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.root));
    }
}

#[test]
fn requires_a_valid_key() -> crate::error::Result<()> {
    let fixture = Fixture::new("missing");
    let (_, presented) = fixture.keys.create("phone", Scope::Use)?;
    assert_eq!(fixture.call("Health", None), Err(Code::Unauthenticated));
    assert_eq!(
        fixture.call("Health", Some("mmk_0011223344556677_wrong")),
        Err(Code::Unauthenticated)
    );
    assert_eq!(
        fixture.call("Health", Some(&presented)).map(|key| key.name),
        Ok("phone".to_owned())
    );
    Ok(())
}

#[test]
fn enforces_the_key_scope() -> crate::error::Result<()> {
    let fixture = Fixture::new("scope");
    let (_, chat) = fixture.keys.create("chat", Scope::Use)?;
    let (_, admin) = fixture.keys.create("admin", Scope::Control)?;
    assert!(fixture.call("Generate", Some(&chat)).is_ok());
    assert_eq!(fixture.call("LoadModel", Some(&chat)), Err(Code::PermissionDenied));
    assert_eq!(fixture.call("PullModel", Some(&chat)), Err(Code::PermissionDenied));
    assert!(fixture.call("LoadModel", Some(&admin)).is_ok());
    assert_eq!(fixture.call("CreateRemoteKey", Some(&admin)), Err(Code::PermissionDenied));
    assert_eq!(fixture.call("NoSuchMethod", Some(&admin)), Err(Code::Unimplemented));
    Ok(())
}

#[test]
fn a_revoked_key_stops_working_and_clients_are_recorded() -> crate::error::Result<()> {
    let fixture = Fixture::new("revoke");
    let (record, presented) = fixture.keys.create("laptop", Scope::Control)?;
    assert!(fixture.call("Telemetry", Some(&presented)).is_ok());
    assert!(fixture.call("ListModels", Some(&presented)).is_ok());
    let recent = fixture.clients.recent();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].requests, 2);
    assert!(fixture.clients.last_used(&record.id).is_some());
    fixture.keys.revoke(&record.id)?;
    assert_eq!(fixture.call("Telemetry", Some(&presented)), Err(Code::Unauthenticated));
    Ok(())
}

#[test]
fn accepts_any_bearer_case_and_rejects_other_schemes() {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("bearer mmk_x"));
    assert_eq!(bearer(&headers), Some("mmk_x"));
    headers.insert("authorization", HeaderValue::from_static("Basic abc"));
    assert_eq!(bearer(&headers), None);
}

#[test]
fn failures_never_lock_out_a_valid_key_from_the_same_network() -> crate::error::Result<()> {
    let fixture = Fixture::new("throttle");
    let (_, presented) = fixture.keys.create("office", Scope::Use)?;
    let wrong = Some("mmk_0011223344556677_wrong");
    for _ in 0..failures::MAX_FAILURES {
        assert_eq!(fixture.call("Health", wrong), Err(Code::Unauthenticated));
    }
    assert_eq!(fixture.call("Health", wrong), Err(Code::ResourceExhausted));
    assert!(fixture.call("Health", Some(&presented)).is_ok(), "a valid key still works");
    Ok(())
}

#[test]
fn refuses_a_connection_without_a_peer_address() -> crate::error::Result<()> {
    let fixture = Fixture::new("peerless");
    let (_, presented) = fixture.keys.create("ghost", Scope::Use)?;
    let refused = fixture.guard.authorize("/mirmir.v1.Runtime/Health", Some(&presented), None);
    assert_eq!(refused.map_err(|status| status.code()).err(), Some(Code::Internal));
    assert!(fixture.clients.recent().is_empty());
    Ok(())
}
