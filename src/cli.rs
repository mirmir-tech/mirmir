use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mirmir", version, about = "Native local model runtime and dashboard")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve(ServeArgs),
    Status,
    Prompt(PromptArgs),
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    #[arg(long)]
    pub no_http: bool,
    #[arg(long, env = "MIRMIR_HTTP_BIND")]
    pub http_bind: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    List,
    Inspect {
        selector: String,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Pull {
        repo_id: String,
        #[arg(long)]
        revision: Option<String>,
    },
    Remove {
        repo_id: String,
    },
    Load {
        selector: String,
        #[arg(long)]
        force: bool,
    },
    Unload {
        selector: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Init,
    Show,
    Edit,
    Validate,
    Set { key: String, value: Option<String> },
    Remove { key: String },
    Test { key: String },
}

#[derive(Debug, Args, Clone)]
pub struct PromptArgs {
    #[arg(long, env = "MIRMIR_MODEL")]
    pub model: Option<String>,
    #[arg(long, conflicts_with_all = ["prompt_file", "stdin"])]
    pub prompt: Option<String>,
    #[arg(long, conflicts_with_all = ["prompt", "stdin"])]
    pub prompt_file: Option<PathBuf>,
    #[arg(long, conflicts_with_all = ["prompt", "prompt_file"])]
    pub stdin: bool,
    #[arg(long, env = "MIRMIR_MAX_TOKENS")]
    pub max_tokens: Option<usize>,
    #[arg(long, env = "MIRMIR_TEMPERATURE")]
    pub temperature: Option<f32>,
    #[arg(long, env = "MIRMIR_TOP_P")]
    pub top_p: Option<f32>,
    #[arg(long, env = "MIRMIR_TOP_K")]
    pub top_k: Option<usize>,
    #[arg(long, env = "MIRMIR_REPETITION_PENALTY")]
    pub repetition_penalty: Option<f32>,
    #[arg(long, env = "MIRMIR_SEED")]
    pub seed: Option<u64>,
    #[arg(long)]
    pub json: bool,
    #[arg(long, value_name = "PATH")]
    pub csv: Option<PathBuf>,
    #[arg(long)]
    pub no_stream: bool,
    #[arg(long, visible_alias = "bench-warmup", default_value_t = 0)]
    pub warmup: usize,
    #[arg(long, visible_alias = "bench-samples", default_value_t = 1)]
    pub samples: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_secrets_through_generic_config_commands() {
        let cli = Cli::try_parse_from(["mirmir", "config", "set", "hugging_face.token"])
            .expect("secret value may be supplied on stdin");
        let Some(Command::Config {
            command: ConfigCommand::Set { key, value },
        }) = cli.command
        else {
            panic!("expected config set");
        };
        assert_eq!(key, "hugging_face.token");
        assert_eq!(value, None);

        assert!(Cli::try_parse_from(["mirmir", "config", "remove", "server.api_key"]).is_ok());
        assert!(Cli::try_parse_from(["mirmir", "config", "test", "hugging_face.token"]).is_ok());
    }

    #[test]
    fn rejects_legacy_secret_subcommands() {
        assert!(Cli::try_parse_from(["mirmir", "config", "set-hf-token"]).is_err());
        assert!(Cli::try_parse_from(["mirmir", "config", "set-http-api-key"]).is_err());
    }
}
