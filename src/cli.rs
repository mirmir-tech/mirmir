use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Run local models, manage their files and configuration, or open the terminal
/// dashboard.
#[derive(Debug, Parser)]
#[command(
    name = "mirmir",
    version,
    about = "Native local model runtime and dashboard",
    long_about = "Run local language, vision, embedding, and reranking models. Without a command, MiRMiR opens its interactive terminal dashboard; commands expose the same runtime for servers, scripts, model management, configuration, and benchmarks.",
    next_line_help = true
)]
pub struct Cli {
    /// Operation to perform; omit it to open the interactive TUI.
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the persistent gRPC runtime and optional HTTP/Web API until
    /// interrupted.
    Serve(ServeArgs),
    /// Print server health, loaded models, and the latest telemetry snapshot as
    /// JSON.
    Status,
    /// Generate from one prompt, optionally repeating it to collect benchmark
    /// statistics.
    Prompt(PromptArgs),
    /// Discover, download, inspect, load, unload, and remove model checkpoints.
    Model {
        /// Model lifecycle or catalog operation to perform.
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Initialize, inspect, validate, and update runtime configuration and
    /// secrets.
    Config {
        /// Configuration operation to perform.
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    /// Disable the HTTP, OpenAI-compatible, and embedded Web endpoints; keep
    /// only local gRPC.
    #[arg(long)]
    pub no_http: bool,
    /// Override the HTTP listen address from config.toml for this process.
    #[arg(long, env = "MIRMIR_HTTP_BIND")]
    pub http_bind: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// List downloaded, external, and partially downloaded local checkpoints as
    /// JSON.
    List,
    /// Inspect task capabilities, generation defaults, and estimated memory
    /// use.
    Inspect {
        /// Local model key, configured alias, repository ID, or checkpoint path
        /// to inspect.
        #[arg(value_name = "SELECTOR")]
        selector: String,
    },
    /// Search the Hugging Face catalog and report compatibility and memory fit
    /// as JSON.
    Search {
        /// Text matched against Hugging Face repository owners and model names.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Maximum number of results returned from the first catalog page.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Download every file from a Hugging Face model repository into the
    /// managed cache.
    Pull {
        /// Hugging Face repository in OWNER/MODEL form.
        #[arg(value_name = "REPO_ID")]
        repo_id: String,
        /// Branch, tag, or commit to download instead of the repository default
        /// revision.
        #[arg(long)]
        revision: Option<String>,
    },
    /// Delete a downloaded model and its persisted metadata; the model must be
    /// unloaded.
    Remove {
        /// Hugging Face repository ID whose managed or external cached files
        /// should be removed.
        #[arg(value_name = "REPO_ID")]
        repo_id: String,
    },
    /// Load a local checkpoint into accelerator memory and persist it for
    /// restoration.
    Load {
        /// Local model key, configured alias, repository ID, or checkpoint path
        /// to load.
        #[arg(value_name = "SELECTOR")]
        selector: String,
        /// Load despite a failed safe-memory preflight; allocation or execution
        /// may still fail.
        #[arg(long)]
        force: bool,
    },
    /// Unload a model from accelerator memory and disable automatic
    /// restoration.
    Unload {
        /// Selector of the currently loaded model to unload.
        #[arg(value_name = "SELECTOR")]
        selector: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Create missing config and secret files with validated defaults.
    Init,
    /// Print effective values, sources, paths, and redacted secret state as
    /// JSON.
    Show,
    /// Edit a temporary config.toml with $VISUAL or $EDITOR and install it
    /// after validation.
    Edit,
    /// Parse and validate the current configuration without changing it.
    Validate,
    /// Set one dotted configuration key or secret, using the running server
    /// when available.
    Set {
        /// Dotted setting name, for example `runtime.kv_blocks` or
        /// `hugging_face.token`.
        #[arg(value_name = "KEY")]
        key: String,
        /// New value; omit it for a supported secret to read the value without
        /// echoing.
        #[arg(value_name = "VALUE")]
        value: Option<String>,
    },
    /// Remove a supported stored secret so environment or default resolution
    /// can take over.
    Remove {
        /// Secret key to remove: `hugging_face.token` or `server.api_key`.
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Verify a supported external credential without displaying its value.
    Test {
        /// Credential key to test; currently `hugging_face.token`.
        #[arg(value_name = "KEY")]
        key: String,
    },
}

#[derive(Debug, Args, Clone)]
pub struct PromptArgs {
    /// Model selector; falls back to `default_model` from config.toml when
    /// omitted.
    #[arg(long, env = "MIRMIR_MODEL")]
    pub model: Option<String>,
    /// Prompt text supplied directly on the command line.
    #[arg(long, conflicts_with_all = ["prompt_file", "stdin"])]
    pub prompt: Option<String>,
    /// Read the UTF-8 prompt from this file.
    #[arg(long, conflicts_with_all = ["prompt", "stdin"])]
    pub prompt_file: Option<PathBuf>,
    /// Read the prompt from standard input until EOF.
    #[arg(long, conflicts_with_all = ["prompt", "prompt_file"])]
    pub stdin: bool,
    /// Maximum number of new tokens generated per sample; model defaults apply
    /// when omitted.
    #[arg(long, env = "MIRMIR_MAX_TOKENS")]
    pub max_tokens: Option<usize>,
    /// Sampling temperature; lower values are more deterministic.
    #[arg(long, env = "MIRMIR_TEMPERATURE")]
    pub temperature: Option<f32>,
    /// Nucleus sampling probability cutoff in the inclusive range from 0 to 1.
    #[arg(long, env = "MIRMIR_TOP_P")]
    pub top_p: Option<f32>,
    /// Restrict sampling to the K most likely tokens; zero leaves the set
    /// unbounded.
    #[arg(long, env = "MIRMIR_TOP_K")]
    pub top_k: Option<usize>,
    /// Penalize repeated tokens; 1 disables the penalty and larger values
    /// reduce repetition.
    #[arg(long, env = "MIRMIR_REPETITION_PENALTY")]
    pub repetition_penalty: Option<f32>,
    /// Seed the sampler so otherwise identical requests can be reproduced.
    #[arg(long, env = "MIRMIR_SEED")]
    pub seed: Option<u64>,
    /// Emit one machine-readable benchmark report instead of human-oriented
    /// output.
    #[arg(long)]
    pub json: bool,
    /// Write per-sample measurements and aggregates to a CSV file.
    #[arg(long, value_name = "PATH")]
    pub csv: Option<PathBuf>,
    /// Buffer the completion and print it only after generation finishes.
    #[arg(long)]
    pub no_stream: bool,
    /// Run this many unreported warmup generations before measured samples.
    #[arg(long, visible_alias = "bench-warmup", default_value_t = 0)]
    pub warmup: usize,
    /// Number of measured generations used for output and aggregate statistics.
    #[arg(long, visible_alias = "bench-samples", default_value_t = 1)]
    pub samples: usize,
}

#[cfg(test)]
mod tests;
