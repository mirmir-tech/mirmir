use std::path::PathBuf;

use clap::Args;

use crate::instance::Instance;

#[derive(Debug, Args)]
pub struct InstanceArgs {
    /// Use /run/mirmir/mirmir.sock; clients only attach and never start a
    /// runtime.
    #[arg(long, global = true, conflicts_with = "socket")]
    pub system: bool,
    /// Select a Unix socket; clients only attach and never use local
    /// configuration.
    #[arg(long, global = true, value_name = "PATH")]
    pub socket: Option<PathBuf>,
}

impl From<InstanceArgs> for Instance {
    fn from(args: InstanceArgs) -> Self {
        if args.system {
            Self::System
        } else if let Some(socket) = args.socket {
            Self::Socket(socket)
        } else {
            Self::Personal
        }
    }
}

#[cfg(test)]
mod tests;
