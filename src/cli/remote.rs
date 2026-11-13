use clap::Subcommand;

use crate::remote::Scope;

#[derive(Debug, Subcommand)]
pub enum RemoteCommand {
    /// Print the TLS listener, certificate fingerprint, API keys and recent
    /// remote clients as JSON.
    Status,
    /// Start the TLS listener now and on every start; creates a first control
    /// key when none exists and prints it once.
    Enable {
        /// Listen address, for example `0.0.0.0:7443` (the default) or one
        /// interface's IP.
        #[arg(long, value_name = "ADDRESS")]
        bind: Option<String>,
    },
    /// Stop the TLS listener and keep remote access off; keys are kept.
    Disable,
    /// Print the SHA-256 fingerprint of the certificate a server presents,
    /// to compare with the server and pin with `--remote-fingerprint`.
    Fingerprint {
        /// Server address in HOST[:PORT] form.
        #[arg(value_name = "HOST[:PORT]")]
        address: String,
    },
    /// Create, list and revoke remote API keys.
    Key {
        /// Key operation to perform.
        #[command(subcommand)]
        command: RemoteKeyCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum RemoteKeyCommand {
    /// Create a key and print it once; the server keeps only its digest.
    Create {
        /// A name that identifies the client, for example `studio-mac`.
        #[arg(long)]
        name: String,
        /// `use` for chat, embeddings and monitoring; `control` also manages
        /// models and settings.
        #[arg(long, value_enum, default_value_t = Scope::Use)]
        scope: Scope,
    },
    /// List keys with their scope and last use, never their values.
    List,
    /// Revoke a key by its id; clients using it are refused immediately.
    Revoke {
        /// The key id, as printed by `mirmir remote key list`.
        #[arg(value_name = "ID")]
        id: String,
    },
}
