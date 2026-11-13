use std::{net::SocketAddr, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The listen address `mirmir remote enable` uses when none is given.
pub const DEFAULT_REMOTE_BIND: &str = "0.0.0.0:7443";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSettings {
    pub http_bind: String,
    pub allow_remote: bool,
    pub web_enabled: bool,
    pub cors_origins: Vec<String>,
    pub body_limit_bytes: usize,
    pub request_timeout_seconds: u64,
    pub max_concurrency: usize,
    /// TLS listen address of the remote gRPC service; absent keeps remote
    /// access off.
    pub remote_bind: Option<String>,
    /// PEM certificate chain for the remote listener; absent uses a
    /// self-signed certificate from the state directory.
    pub remote_tls_cert: Option<PathBuf>,
    /// PEM private key matching `remote_tls_cert`.
    pub remote_tls_key: Option<PathBuf>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            http_bind: "127.0.0.1:8080".to_owned(),
            allow_remote: false,
            web_enabled: false,
            cors_origins: Vec::new(),
            body_limit_bytes: crate::media::DEFAULT_IMAGE_REQUEST_BYTES,
            request_timeout_seconds: 300,
            max_concurrency: 16,
            remote_bind: None,
            remote_tls_cert: None,
            remote_tls_key: None,
        }
    }
}

impl ServerSettings {
    pub fn address(&self) -> Result<SocketAddr> {
        match self.http_bind.parse() {
            Ok(address) => Ok(address),
            Err(error) => Err(Error::Config(format!(
                "server.http_bind `{}` is invalid: {error}",
                self.http_bind
            ))),
        }
    }

    /// The remote listen address, or `None` when remote access is off.
    pub fn remote_address(&self) -> Result<Option<SocketAddr>> {
        self.remote_bind.as_deref().map(parse_remote).transpose()
    }

    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds)
    }

    pub fn validate(&self) -> Result<()> {
        let address = self.address()?;
        if !address.ip().is_loopback() && !self.allow_remote {
            return Err(Error::Config(
                "server.allow_remote must be true for a non-loopback HTTP bind".to_owned(),
            ));
        }
        if self.web_enabled && !address.ip().is_loopback() {
            return Err(Error::Config(
                "server.web_enabled currently requires a loopback HTTP bind".to_owned(),
            ));
        }
        if self.body_limit_bytes == 0 || self.request_timeout_seconds == 0 {
            return Err(Error::Config("HTTP body limit and timeout must be positive".to_owned()));
        }
        if self.max_concurrency == 0 {
            return Err(Error::Config("server.max_concurrency must be positive".to_owned()));
        }
        self.remote_address()?;
        if self.remote_tls_cert.is_some() != self.remote_tls_key.is_some() {
            return Err(Error::Config(
                "server.remote_tls_cert and server.remote_tls_key must be set together".to_owned(),
            ));
        }
        Ok(())
    }
}

fn parse_remote(bind: &str) -> Result<SocketAddr> {
    bind.parse()
        .map_err(|error| Error::Config(format!("server.remote_bind `{bind}` is invalid: {error}")))
}
