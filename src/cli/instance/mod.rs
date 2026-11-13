use std::path::PathBuf;

use clap::Args;

use crate::{
    error::{Error, Result},
    instance::Instance,
    remote::{RemoteTarget, Trust},
};

/// The port `--remote` uses when the address has none.
const DEFAULT_REMOTE_PORT: u16 = 7443;

#[derive(Debug, Args)]
pub struct InstanceArgs {
    /// Use /run/mirmir/mirmir.sock; clients only attach and never start a
    /// runtime.
    #[arg(long, global = true, conflicts_with_all = ["socket", "remote"])]
    pub system: bool,
    /// Select a Unix socket; clients only attach and never use local
    /// configuration.
    #[arg(long, global = true, value_name = "PATH", conflicts_with = "remote")]
    pub socket: Option<PathBuf>,
    /// Attach to another computer's mirmir over TLS at HOST[:PORT] (default
    /// port 7443); requires a remote API key.
    #[arg(long, global = true, value_name = "HOST[:PORT]")]
    pub remote: Option<String>,
    /// Remote API key (`mmk_…`) sent with every call; prefer the environment
    /// variable over the command line.
    #[arg(long, global = true, env = "MIRMIR_REMOTE_KEY", hide_env_values = true)]
    pub remote_key: Option<String>,
    /// Pin the server certificate by its SHA-256 fingerprint, as shown by
    /// `mirmir remote status` on the server; omit it for a CA-issued
    /// certificate.
    #[arg(long, global = true, env = "MIRMIR_REMOTE_FINGERPRINT", value_name = "SHA256")]
    pub remote_fingerprint: Option<String>,
}

impl TryFrom<InstanceArgs> for Instance {
    type Error = Error;

    fn try_from(args: InstanceArgs) -> Result<Self> {
        if args.system {
            return Ok(Self::System);
        }
        if let Some(socket) = args.socket {
            return Ok(Self::Socket(socket));
        }
        let Some(address) = args.remote else {
            return Ok(Self::Personal);
        };
        let key = args.remote_key.filter(|key| !key.trim().is_empty()).ok_or_else(|| {
            Error::Config("--remote requires --remote-key or MIRMIR_REMOTE_KEY".to_owned())
        })?;
        let trust = match args.remote_fingerprint.filter(|value| !value.trim().is_empty()) {
            Some(fingerprint) => Trust::Pinned(fingerprint.parse()?),
            None => Trust::System,
        };
        Ok(Self::Remote(RemoteTarget { address: with_port(&address), key, trust }))
    }
}

/// Appends the default port unless the address already names one.
pub fn with_port(address: &str) -> String {
    let address = address.trim();
    let has_port = address
        .strip_prefix('[')
        .map_or_else(|| address.matches(':').count() == 1, |rest| rest.contains("]:"));
    if has_port {
        address.to_owned()
    } else if address.contains(':') && !address.starts_with('[') {
        format!("[{address}]:{DEFAULT_REMOTE_PORT}")
    } else {
        format!("{address}:{DEFAULT_REMOTE_PORT}")
    }
}

#[cfg(test)]
mod tests;
