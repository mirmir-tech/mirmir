mod args;
mod chat;
mod error;
mod inspect;
#[cfg(target_os = "macos")]
mod trace;
mod ui;

use std::{
    hint::black_box,
    io::{self, Write},
    time::Instant,
};

use args::{Cli, Command};
use clap::Parser;
use error::CliError;
use libmir::{
    RuntimeConfig,
    runtime::kv::{BlockId, BlockTable, KvStorageSpec, KvWritePlan},
};
use uuid::Uuid;

const DEFAULT_LOG_FILTER: &str = "cli=info,cuda=info,metal=info,runtime=info,models=info";

fn main() -> Result<(), CliError> {
    drop(config::load_environment(std::env::current_dir()?)?);
    init_tracing()?;
    let cli = Cli::parse();
    let config = cli.runtime.runtime_config();
    match cli.command {
        Command::Chat(args) => chat::run(&config, args),
        Command::Inspect { path } => inspect::run(&path),
        #[cfg(target_os = "macos")]
        Command::TraceMetal(args) => {
            write_lines(ui::trace_report(&trace::trace_mlx_model(&config, &args)?))
        },
        Command::BenchKv {
            tokens,
            iterations,
            layers,
            kv_heads,
            head_dim,
            native_bits,
        } => bench_kv(&config, tokens, iterations, layers, kv_heads, head_dim, native_bits),
        Command::ServeInfo => serve_info(&config, &cli.http),
    }
}

fn serve_info(config: &RuntimeConfig, http: &config::HttpArgs) -> Result<(), CliError> {
    write_lines([
        format!("mirmir server defaults to http://{}", http.bind_addr()),
        "openai-like route: /v1/chat/completions".into(),
        "websocket route: /v1/ws".into(),
        format!(
            "kv cache: dtype {}, block_size {}, blocks {}",
            config.kv_cache.dtype, config.kv_cache.block_size, config.kv_cache.block_count
        ),
    ])
}

fn init_tracing() -> Result<(), CliError> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_LOG_FILTER));
    match tracing_subscriber::fmt().with_env_filter(env_filter).try_init() {
        Ok(()) => Ok(()),
        Err(error) => Err(CliError::Message(format!("tracing init failed: {error}"))),
    }
}

fn bench_kv(
    config: &RuntimeConfig,
    tokens: usize,
    iterations: usize,
    layers: usize,
    kv_heads: usize,
    head_dim: usize,
    native_bits: u8,
) -> Result<(), CliError> {
    let table = block_table(tokens, config.kv_cache.block_size)?;
    let started = Instant::now();
    let mut planned_tokens = 0;
    let mut planned_writes = 0;
    for iteration in 0..iterations {
        for layer in 0..layers {
            let plan = KvWritePlan::prefill(Uuid::nil(), layer, &table, 0, tokens)?;
            planned_tokens += black_box(plan.written_tokens());
            planned_writes += black_box(plan.writes().len());
        }
        black_box(iteration);
    }
    let elapsed = started.elapsed();
    let spec = KvStorageSpec {
        cache: config.kv_cache,
        layout: libmir::runtime::kv::KvCacheLayout::Nhd,
        kv_heads,
        key_head_dim: head_dim,
        value_head_dim: head_dim,
        native_bits,
    };
    let budget = spec.memory_budget();
    let runs = iterations.saturating_mul(layers).max(1);
    write_lines([
        format!("kv dtype: {} ({:?})", config.kv_cache.dtype, config.kv_cache.dtype.quant_mode()),
        format!("tokens: {tokens}, layers: {layers}, iterations: {iterations}"),
        format!(
            "block_size: {}, blocks_used: {}",
            config.kv_cache.block_size,
            table.blocks().len()
        ),
        format!("planned_writes: {planned_writes}, planned_tokens: {planned_tokens}"),
        format!("plan_ns_per_layer: {}", elapsed.as_nanos() / runs as u128),
        format!("data_bytes_per_token: {}", budget.data_bytes_per_token),
        format!("scale_bytes_per_token: {}", budget.scale_bytes_per_token),
        format!("bytes_per_block: {}", budget.bytes_per_block),
        format!("configured_total_kv_bytes: {}", budget.total_bytes),
    ])
}

fn block_table(tokens: usize, block_size: usize) -> Result<BlockTable, CliError> {
    let mut table = BlockTable::with_block_size(block_size);
    for id in 0..tokens.div_ceil(block_size.max(1)) {
        table.push(BlockId(u32::try_from(id)?));
    }
    table.set_token_len(tokens);
    Ok(table)
}

fn write_lines(lines: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for line in lines {
        writeln!(handle, "{line}")?;
    }
    Ok(())
}
