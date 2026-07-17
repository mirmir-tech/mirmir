use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "mirmir-server")]
#[command(about = "OpenAI-like HTTP/WebSocket server for mirmir")]
pub struct ServerArgs {
    #[arg(long, env = "MODEL")]
    pub model: Option<PathBuf>,
    #[command(flatten)]
    pub runtime: config::RuntimeArgs,
    #[command(flatten)]
    pub http: config::HttpArgs,
}
