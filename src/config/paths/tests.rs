use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn paths() -> Paths {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = env::temp_dir().join(format!("mirmir-paths-{}-{id}", std::process::id()));
    Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"))
}

#[test]
fn socket_override_uses_its_own_lock_and_keeps_private_storage() -> Result<()> {
    let personal = paths();
    let socket = personal.state_dir.parent().expect("root").join("shared/runtime.sock");
    let external = personal.clone().with_socket(socket.clone(), SocketAccess::Group)?;
    assert_eq!(external.socket_file, socket);
    assert_eq!(external.lock_file, socket.with_file_name("runtime.lock"));
    assert_eq!(external.config_file, personal.config_file);
    assert_eq!(external.state_dir, personal.state_dir);
    assert!(
        personal
            .clone()
            .with_socket(personal.state_dir.join("socket"), SocketAccess::Group)
            .is_err()
    );
    Ok(())
}

#[test]
fn selecting_the_personal_socket_preserves_its_instance_lock() -> Result<()> {
    let personal = paths();
    let explicit = personal
        .clone()
        .with_socket(personal.socket_file.clone(), SocketAccess::Owner)?;
    assert_eq!(explicit.lock_file, personal.lock_file);
    Ok(())
}

#[cfg(unix)]
#[test]
fn shared_directory_is_traversable_but_private_storage_stays_private() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let personal = paths();
    let socket = personal.state_dir.parent().expect("root").join("shared/runtime.sock");
    let external = personal.with_socket(socket, SocketAccess::Group)?;
    external.ensure_runtime_dirs()?;
    external.ensure_config_dirs()?;
    for (path, expected) in [
        (external.socket_file.parent().expect("parent"), 0o750),
        (external.state_dir.as_path(), 0o700),
        (external.config_dir.as_path(), 0o700),
    ] {
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, expected);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn never_chmods_an_existing_external_parent() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let personal = paths();
    let parent = personal.state_dir.parent().expect("root").join("preexisting");
    fs::create_dir_all(&parent)?;
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755))?;
    let external = personal.with_socket(parent.join("socket"), SocketAccess::Owner)?;
    external.ensure_runtime_dirs()?;
    assert_eq!(fs::metadata(parent)?.permissions().mode() & 0o777, 0o755);
    Ok(())
}
