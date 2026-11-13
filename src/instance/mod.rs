use std::path::{Path, PathBuf};

use crate::{
    config::{Paths, SocketAccess, Store},
    daemon::{self, Connection},
    error::{Error, Result},
    remote::RemoteTarget,
    rpc,
};

pub const SYSTEM_SOCKET: &str = "/run/mirmir/mirmir.sock";

mod endpoint;
pub use endpoint::{Address, Endpoint};

#[derive(Debug, Clone, Default)]
pub enum Instance {
    #[default]
    Personal,
    System,
    Socket(PathBuf),
    /// A remote server's TLS listener; clients only attach.
    Remote(RemoteTarget),
}

impl Instance {
    /// Whether clients only attach to an existing server and never start or
    /// configure a local one.
    pub const fn is_attached(&self) -> bool {
        !matches!(self, Self::Personal)
    }

    pub fn attached_socket(&self) -> Option<&Path> {
        match self {
            Self::Personal | Self::Remote(_) => None,
            Self::System => Some(Path::new(SYSTEM_SOCKET)),
            Self::Socket(socket) => Some(socket),
        }
    }

    pub async fn connect(&self) -> Result<Connection> {
        if let Self::Remote(target) = self {
            let client = rpc::connect_remote(target).await?;
            return Ok(Connection::attached(client, Address::Remote(target.address.clone())));
        }
        if let Some(socket) = self.attached_socket() {
            Ok(Connection::attached(
                self.connect_existing().await?,
                Address::Socket(socket.to_owned()),
            ))
        } else {
            let paths = Paths::discover()?;
            let config = Store::new(paths.clone()).load()?;
            daemon::connect_or_start(&paths, &config).await
        }
    }

    pub async fn connect_existing(&self) -> Result<rpc::Client> {
        if let Self::Remote(target) = self {
            return rpc::connect_remote(target).await;
        }
        let socket = self
            .attached_socket()
            .ok_or_else(|| Error::Config("an explicit server socket is required".to_owned()))?;
        rpc::connect(socket).await.map_err(|source| Error::Attach {
            socket: socket.to_owned(),
            source: Box::new(source),
        })
    }

    pub fn server_paths(&self, shared: bool) -> Result<Paths> {
        if matches!(self, Self::Remote(_)) {
            return Err(Error::Config("mirmir serve runs locally; drop --remote".to_owned()));
        }
        let paths = Paths::discover()?;
        if let Some(socket) = self.attached_socket() {
            let access = if shared || matches!(self, Self::System) {
                SocketAccess::Group
            } else {
                SocketAccess::Owner
            };
            paths.with_socket(socket.to_owned(), access)
        } else if shared {
            Err(Error::Config("--shared-socket requires --system or --socket".to_owned()))
        } else {
            Ok(paths)
        }
    }
}

#[cfg(test)]
mod tests;
