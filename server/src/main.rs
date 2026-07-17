mod args;

use args::ServerArgs;
use clap::Parser;
use libmir::{GenerationOverrides, Library};
use server::{error::ServerError, router, state::AppState};

const DEFAULT_LOG_FILTER: &str =
    "server=info,tower_http=info,cuda=info,metal=info,runtime=info,models=info";

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    drop(config::load_environment(std::env::current_dir()?)?);
    let args = ServerArgs::parse();
    let config = args.runtime.runtime_config();
    let addr = args.http.bind_addr();
    let model_path = args.model.clone();

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_LOG_FILTER));

    tracing_subscriber::fmt().with_env_filter(env_filter).try_init()?;

    let library = Library::new(config.clone());
    let model = model_path
        .map(|path| {
            library.load(path, GenerationOverrides::default(), &mut |event| {
                tracing::info!(
                    current = event.current,
                    total = event.total,
                    detail = %event.detail,
                    "loading server model"
                );
            })
        })
        .transpose()?;
    let state = AppState::new(library, model);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(
        "mirmir server listening on http://{addr}; KV cache dtype {}, block_size {}, blocks {}",
        config.kv_cache.dtype,
        config.kv_cache.block_size,
        config.kv_cache.block_count
    );
    Ok(axum::serve(listener, router(state)).await?)
}
