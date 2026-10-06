use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;
use crate::{
    cli::ConfigCommand,
    config::{AppConfig, command},
    rpc::proto,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn paths() -> Paths {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mirmir-attach-{}-{id}", std::process::id()));
    Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"))
}

#[tokio::test]
async fn missing_explicit_server_never_creates_storage_or_starts_runtime() {
    let paths = paths();
    let instance = Instance::Socket(paths.socket_file.clone());
    assert!(matches!(instance.connect().await, Err(Error::Attach { .. })));
    assert!(crate::status::run(&instance).await.is_err());
    assert!(crate::model::run(&instance, crate::cli::ModelCommand::List).await.is_err());
    assert!(!paths.socket_file.parent().expect("parent").exists());
    assert!(!paths.config_dir.exists());
    assert!(!paths.state_dir.exists());
}

#[tokio::test]
async fn attaches_to_existing_runtime_and_preserves_it_on_disconnect() -> Result<()> {
    let paths = paths();
    let owner = daemon::connect_or_start(&paths, &AppConfig::default()).await?;
    let instance = Instance::Socket(paths.socket_file.clone());
    let mut attached = instance.connect().await?;
    assert!(attached.reused());
    attached.client.telemetry(proto::TelemetryRequest {}).await?;
    attached.shutdown().await?;
    instance.connect_existing().await?.health(proto::HealthRequest {}).await?;
    owner.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn group_socket_exposes_server_configuration_through_rpc() -> Result<()> {
    let paths = paths();
    let socket = paths.state_dir.parent().expect("root").join("shared/runtime.sock");
    let paths = paths.with_socket(socket.clone(), SocketAccess::Group)?;
    let store = Store::new(paths.clone());
    store.initialize()?;
    let owner = daemon::connect_or_start(&paths, &store.load()?).await?;
    let instance = Instance::Socket(socket);
    command::run(
        &instance,
        ConfigCommand::Set {
            key: "server.max_concurrency".to_owned(),
            value: Some("7".to_owned()),
        },
    )
    .await?;
    assert_eq!(store.load()?.server.max_concurrency, 7);
    let config = instance
        .connect_existing()
        .await?
        .get_configuration(proto::GetConfigurationRequest {})
        .await?
        .into_inner();
    assert_eq!(config.config_path, paths.config_file.display().to_string());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&paths.socket_file)?.permissions().mode() & 0o777, 0o660);
        assert_eq!(fs::metadata(&paths.lock_file)?.permissions().mode() & 0o777, 0o600);
    }
    owner.shutdown().await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn permission_denied_does_not_start_a_runtime_or_edit_configuration() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let paths = paths();
    let store = Store::new(paths.clone());
    store.initialize()?;
    let original = fs::read(&paths.config_file)?;
    let owner = daemon::connect_or_start(&paths, &store.load()?).await?;
    let instance = Instance::Socket(paths.socket_file.clone());
    let parent = paths.socket_file.parent().expect("parent");
    fs::set_permissions(parent, fs::Permissions::from_mode(0o000))?;
    let denied = match instance.connect().await {
        Err(error) => error,
        Ok(connection) => {
            // Privileged test runners can bypass directory access checks.
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            connection.shutdown().await?;
            owner.shutdown().await?;
            return Ok(());
        },
    };
    let updated = command::run(
        &instance,
        ConfigCommand::Set {
            key: "server.max_concurrency".to_owned(),
            value: Some("7".to_owned()),
        },
    )
    .await;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let mut client = instance.connect_existing().await?;
    client.health(proto::HealthRequest {}).await?;
    owner.shutdown().await?;
    assert!(
        matches!(denied, Error::Attach { source, .. } if matches!(*source, Error::Io(ref error) if error.kind() == std::io::ErrorKind::PermissionDenied))
    );
    assert!(matches!(updated, Err(Error::Attach { .. })));
    assert_eq!(fs::read(paths.config_file)?, original);
    Ok(())
}

#[tokio::test]
async fn non_socket_target_is_reported_without_replacing_the_file() -> Result<()> {
    let paths = paths();
    fs::create_dir_all(paths.socket_file.parent().expect("parent"))?;
    fs::write(&paths.socket_file, "existing file")?;
    let instance = Instance::Socket(paths.socket_file.clone());
    assert!(
        matches!(instance.connect().await, Err(Error::Attach { source, .. }) if matches!(*source, Error::NotSocket(_)))
    );
    assert_eq!(fs::read_to_string(paths.socket_file)?, "existing file");
    assert!(!paths.lock_file.exists());
    assert!(!paths.state_dir.exists());
    Ok(())
}
