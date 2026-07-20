use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mirmir")]
#[command(about = "High-performance native Rust LLM runtime CLI")]
pub struct Cli {
    #[command(flatten)]
    pub runtime: config::RuntimeArgs,
    #[command(flatten)]
    pub http: config::HttpArgs,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Chat(ChatArgs),
    Inspect {
        #[arg(long)]
        path: PathBuf,
    },
    #[cfg(target_os = "macos")]
    TraceMetal(TraceMetalArgs),
    BenchKv {
        #[arg(long, default_value_t = 8192)]
        tokens: usize,
        #[arg(long, default_value_t = 1000)]
        iterations: usize,
        #[arg(long, default_value_t = 1)]
        layers: usize,
        #[arg(long, default_value_t = 8)]
        kv_heads: usize,
        #[arg(long, default_value_t = 128)]
        head_dim: usize,
        #[arg(long, default_value_t = 16)]
        native_bits: u8,
    },
    ServeInfo,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Args)]
pub struct TraceMetalArgs {
    #[arg(long)]
    pub path: PathBuf,
    #[arg(long, default_value = "trace")]
    pub model_id: String,
    #[arg(long, default_value = "0,1", conflicts_with = "prompt")]
    pub tokens: String,
    #[arg(long)]
    pub prompt: Option<String>,
    #[arg(long, default_value_t = 1.0e-3)]
    pub max_diff: f32,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct ChatArgs {
    #[arg(long, env = "MODEL")]
    pub model: PathBuf,
    #[arg(long)]
    pub prompt: String,
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
    #[arg(long, env = "MIRMIR_VERBOSE")]
    pub verbose: bool,
    #[arg(long, env = "MIRMIR_TRACE")]
    pub trace: bool,
    #[arg(long, env = "MIRMIR_REVIEW")]
    pub review: bool,
    #[arg(long, env = "MIRMIR_BENCH")]
    pub bench: bool,
    #[arg(long, env = "MIRMIR_BENCH_WARMUP", default_value_t = 1)]
    pub bench_warmup: usize,
    #[arg(long, env = "MIRMIR_BENCH_SAMPLES", default_value_t = 3)]
    pub bench_samples: usize,
}
