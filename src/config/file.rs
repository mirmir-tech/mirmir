use std::{
    fs,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::error::Result;

pub fn write_toml(path: &Path, value: &impl serde::Serialize, private: bool) -> Result<()> {
    write_text(path, &toml::to_string_pretty(value)?, private)
}

/// Distinguishes concurrent temporary files of one process.
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Replaces `path` atomically: each call writes and syncs its own temporary
/// file, unique to this process and call, then renames it into place.
pub fn write_text(path: &Path, contents: &str, private: bool) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        crate::error::Error::Config(format!("{} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let written = replace(&temporary, path, contents, private);
    if written.is_err() {
        drop(fs::remove_file(&temporary));
    }
    written?;
    sync_dir(parent)
}

fn replace(temporary: &Path, path: &Path, contents: &str, private: bool) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    set_mode(&mut options, private);
    let mut file = options.open(temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(options: &mut fs::OpenOptions, private: bool) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(if private {
        0o600
    } else {
        0o644
    });
}

#[cfg(not(unix))]
fn set_mode(_options: &mut fs::OpenOptions, _private: bool) {}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn ensure_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode == 0o600 {
        Ok(())
    } else {
        Err(crate::error::Error::Config(format!(
            "{} must have permissions 0600, found {mode:04o}",
            path.display()
        )))
    }
}

#[cfg(not(unix))]
pub fn ensure_private_file(_path: &Path) -> Result<()> {
    Ok(())
}
