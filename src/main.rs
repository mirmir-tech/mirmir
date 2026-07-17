mod catalog;
mod cli;
mod config;
mod config_command;
mod daemon;
mod error;
mod http;
mod model;
mod output;
mod prompt;
mod rpc;
mod status;
mod tui;
mod web;

use clap::Parser;
use cli::{Cli, Command};
use error::Result;

#[tokio::main]
async fn main() -> Result<()> {
    config::load_dotenv(&std::env::current_dir()?)?;
    let cli = Cli::parse();
    init_tracing(matches!(cli.command.as_ref(), Some(Command::Serve(_))))?;
    let paths = config::Paths::discover()?;
    let store = config::Store::new(paths.clone());

    match cli.command {
        Some(Command::Serve(args)) => daemon::serve(paths, store.load()?, args).await,
        Some(Command::Status) => status::run(&paths).await,
        Some(Command::Prompt(args)) => prompt::run(paths, store.load()?, args).await,
        Some(Command::Model { command }) => model::run(paths, store.load()?, command).await,
        Some(Command::Config { command }) => config_command::run(&paths, &store, command).await,
        None => tui::run(paths, store.load()?).await,
    }
}

fn init_tracing(serve: bool) -> Result<()> {
    let rust_log = std::env::var("RUST_LOG").ok();
    let filter =
        tracing_subscriber::EnvFilter::new(tracing_filter_spec(serve, rust_log.as_deref()));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(|error| error::Error::Config(format!("tracing initialization failed: {error}")))?;
    Ok(())
}

fn tracing_filter_spec(serve: bool, rust_log: Option<&str>) -> String {
    let configured = rust_log.filter(|value| !value.trim().is_empty());
    if serve {
        return configured.map_or_else(
            || default_tracing_filter(true).to_owned(),
            |value| format!("{},{value}", default_tracing_filter(true)),
        );
    }
    configured.map_or_else(|| default_tracing_filter(false).to_owned(), str::to_owned)
}

const fn default_tracing_filter(serve: bool) -> &'static str {
    if serve {
        "mirmir=info,libmir=info,metal=info,cuda=info,tower_http=info"
    } else {
        "mirmir=warn,libmir=warn,metal=warn,cuda=warn"
    }
}

#[cfg(test)]
mod tests {
    use super::{default_tracing_filter, tracing_filter_spec};

    #[test]
    fn serve_enables_info_logs_by_default() {
        assert!(default_tracing_filter(true).contains("mirmir=info"));
        assert!(default_tracing_filter(true).contains("libmir=info"));
        assert!(default_tracing_filter(true).contains("tower_http=info"));
        assert!(default_tracing_filter(false).contains("mirmir=warn"));
    }

    #[test]
    fn serve_keeps_its_info_baseline_with_a_global_filter() {
        let filter = tracing_filter_spec(true, Some("warn"));
        assert!(filter.starts_with("mirmir=info,libmir=info"));
        assert!(filter.ends_with(",warn"));
        assert_eq!(tracing_filter_spec(false, Some("debug")), "debug");
    }
}
