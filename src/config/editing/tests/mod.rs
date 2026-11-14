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
    store.set_remote_bind(Some("0.0.0.0:7443"))?;
    assert_eq!(store.load()?.server.remote_bind.as_deref(), Some("0.0.0.0:7443"));
    assert!(store.set_remote_bind(Some("not an address")).is_err());
    store.set_remote_bind(None)?;
    assert_eq!(store.load()?.server.remote_bind, None);
    Ok(())
}

#[test]
fn generic_edits_cannot_change_the_remote_listener() -> Result<()> {
    let store = store();
    store.initialize()?;
    for input in ["0.0.0.0:7443", "off"] {
        let refused = store
            .set_config_value("server.remote_bind", input)
            .expect_err("the listener address changes only with the listener");
        assert!(refused.to_string().contains("mirmir remote enable"), "{refused}");
    }
    assert_eq!(store.load()?.server.remote_bind, None);
    let presented = store.configuration()?;
    let bind = presented.values.iter().find(|value| value.key == "server.remote_bind");
    assert!(bind.is_some_and(|value| !value.editable && value.value == "off"));
    Ok(())
}

#[test]
fn concurrent_edits_of_different_keys_all_persist() -> Result<()> {
    let store = store();
    store.initialize()?;
    let keys = [
        ("runtime.kv_block_size", "32"),
        ("runtime.kv_blocks", "64"),
        ("runtime.max_batch_requests", "3"),
        ("runtime.max_batch_tokens", "512"),
        ("runtime.decode_priority_burst", "2"),
        ("runtime.prefill_batch_wait_us", "40"),
        ("default_model", "chosen"),
    ];
    std::thread::scope(|scope| {
        for (key, value) in keys {
            let store = store.clone();
            scope.spawn(move || store.set_config_value(key, value).expect("edit"));
        }
    });
    let config = store.load()?;
    assert_eq!(config.runtime.kv_block_size, Some(32));
    assert_eq!(config.runtime.kv_blocks, Some(64));
    assert_eq!(config.runtime.max_batch_requests, Some(3));
    assert_eq!(config.runtime.max_batch_tokens, Some(512));
    assert_eq!(config.runtime.decode_priority_burst, Some(2));
    assert_eq!(config.runtime.prefill_batch_wait_us, Some(40));
    assert_eq!(config.default_model.as_deref(), Some("chosen"));
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
