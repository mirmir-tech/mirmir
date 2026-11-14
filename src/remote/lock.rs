//! An exclusive advisory lock beside a file in the state directory. It
//! serializes complete read/modify/write transactions across processes;
//! `flock` also excludes other open file descriptions within this process.

use std::{
    fs,
    path::{Path, PathBuf},
};

use fs2::FileExt;

use crate::error::{Error, RemoteError, Result};

pub struct FileLock {
    file: fs::File,
}

impl FileLock {
    /// Blocks until this process holds the lock for `path`, kept in a hidden
    /// `.name.lock` file next to it.
    pub fn acquire(path: &Path) -> Result<Self> {
        let lock = lock_path(path)?;
        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(lock)?;
        FileExt::lock_exclusive(&file)?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        drop(FileExt::unlock(&self.file));
    }
}

fn lock_path(path: &Path) -> Result<PathBuf> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(Error::Remote(RemoteError::Failed(format!(
            "{} cannot have a lock file beside it",
            path.display()
        ))));
    };
    fs::create_dir_all(parent)?;
    Ok(parent.join(format!(".{}.lock", name.to_string_lossy())))
}
