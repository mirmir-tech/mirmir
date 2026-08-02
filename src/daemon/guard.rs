use std::{fs, path::Path};

use fs2::FileExt;

use crate::error::{Error, Result};

pub struct InstanceGuard {
    file: fs::File,
}

impl InstanceGuard {
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        if let Err(error) = FileExt::try_lock_exclusive(&file) {
            return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                Error::AlreadyRunning(path.to_owned())
            } else {
                Error::Io(error)
            });
        }
        Ok(Self { file })
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        drop(FileExt::unlock(&self.file));
    }
}
