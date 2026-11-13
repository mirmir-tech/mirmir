mod metal;
mod refill;
mod reset;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::config::{Paths, RemoteCertificate};

static NEXT_EDIT: AtomicU64 = AtomicU64::new(0);

#[test]
fn patches_one_value_without_losing_comments() -> Result<()> {
    let store = store();
    store.paths.ensure_config_dirs()?;
    fs::write(
        &store.paths.config_file,
        "# keep this comment\nschema_version = 1\ndefault_model = \"old\"\n",
    )?;
    store.set_config_value("default_model", "new")?;
    let rendered = fs::read_to_string(&store.paths.config_file)?;
    assert!(rendered.contains("# keep this comment"));
    assert!(rendered.contains("default_model = \"new\""));
    assert_eq!(store.load()?.default_model.as_deref(), Some("new"));
    Ok(())
}

#[test]
fn validates_before_replacing_the_file() -> Result<()> {
    let store = store();
    store.initialize()?;
    let before = fs::read_to_string(&store.paths.config_file)?;
    assert!(store.set_config_value("server.allow_remote", "perhaps").is_err());
    assert_eq!(fs::read_to_string(&store.paths.config_file)?, before);
    Ok(())
}

#[test]
fn enables_the_embedded_web_foundation() -> Result<()> {
    let store = store();
    store.initialize()?;
    store.set_config_value("server.web_enabled", "true")?;
    assert!(store.load()?.server.web_enabled);
    Ok(())
}

#[test]
fn selects_int8_kv_storage() -> Result<()> {
    let store = store();
    store.initialize()?;
    store.set_config_value("runtime.kv_cache_dtype", "int8_per_token_head")?;
    assert_eq!(store.load()?.runtime.kv_cache_dtype, Some(KvCacheDType::Int8PerTokenHead));
    store.set_config_value("runtime.kv_cache_dtype", "auto")?;
    assert_eq!(store.load()?.runtime.kv_cache_dtype, None);
    Ok(())
}

fn store() -> Store {
    let id = NEXT_EDIT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-edit-{}-{id}", std::process::id()));
    Store::new(Paths::from_roots(
        root.join("config"),
        root.join("state"),
        &root.join("runtime"),
    ))
}

#[test]
fn sets_and_clears_the_remote_listener() -> Result<()> {
    let store = store();
    store.initialize()?;
    store.set_config_value("server.remote_bind", "0.0.0.0:7443")?;
    assert_eq!(store.load()?.server.remote_bind.as_deref(), Some("0.0.0.0:7443"));
    assert!(store.set_config_value("server.remote_bind", "not an address").is_err());
    store.set_config_value("server.remote_bind", "off")?;
    assert_eq!(store.load()?.server.remote_bind, None);
    Ok(())
}

#[test]
fn sets_and_resets_the_remote_certificate_pair_one_key_at_a_time() -> Result<()> {
    for (first, second) in [
        ("server.remote_tls_cert", "server.remote_tls_key"),
        ("server.remote_tls_key", "server.remote_tls_cert"),
    ] {
        let store = store();
        let value = |key: &str| {
            if key.ends_with("cert") {
                "/etc/cert.pem"
            } else {
                "/etc/key.pem"
            }
        };
        store.set_config_value(first, value(first))?;
        assert!(store.load()?.server.remote_certificate().is_err(), "a half pair is refused");
        store.set_config_value(second, value(second))?;
        let server = store.load()?.server;
        assert_eq!(
            server.remote_certificate()?,
            RemoteCertificate::Configured {
                certificate: Path::new("/etc/cert.pem"),
                key: Path::new("/etc/key.pem"),
            }
        );
        store.set_config_value(first, "auto")?;
        assert!(store.load()?.server.remote_certificate().is_err());
        store.set_config_value(second, "auto")?;
        assert_eq!(store.load()?.server.remote_certificate()?, RemoteCertificate::SelfSigned);
    }
    Ok(())
}
