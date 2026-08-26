mod connection;
mod guard;
mod server;
mod telemetry;

pub use connection::connect_or_start;

use crate::{
    application::Application,
    cli::ServeArgs,
    config::{AppConfig, Paths, Store},
    error::Result,
    http,
    rpc::RuntimeService,
};

pub async fn serve(paths: Paths, mut config: AppConfig, args: ServeArgs) -> Result<()> {
    if let Some(bind) = args.http_bind {
        config.server.http_bind = bind;
        config.validate()?;
    }
    tracing::info!(
        http_enabled = !args.no_http,
        http_bind = %config.server.http_bind,
        socket = %paths.socket_file.display(),
        "starting mirmir server"
    );
    let store = Store::new(paths.clone());
    let api_key = store.http_api_key()?;
    if !args.no_http && !config.server.address()?.ip().is_loopback() && api_key.is_none() {
        return Err(crate::error::Error::Config(
            "a non-loopback HTTP bind requires an API key".to_owned(),
        ));
    }
    let application = Application::new(&config, store);
    let service = RuntimeService::from_application(application.clone());
    let owner = server::start_service(paths.clone(), service.clone())?;
    tracing::info!(socket = %paths.socket_file.display(), "gRPC server listening");
    if args.no_http {
        restore_in_background(service.clone());
        shutdown_signal().await?;
        tracing::info!("shutdown signal received");
        return owner.shutdown().await;
    }
    let http = match http::start(application, &config.server, api_key).await {
        Ok(http) => http,
        Err(error) => {
            if let Err(cleanup) = owner.shutdown().await {
                tracing::error!(%cleanup, "failed to clean up gRPC after HTTP startup error");
            }
            return Err(error);
        },
    };
    tracing::info!(
        address = %http.address(),
        web_enabled = config.server.web_enabled,
        "HTTP server listening"
    );
    restore_in_background(service);
    shutdown_signal().await?;
    tracing::info!("shutdown signal received");
    let http_result = http.shutdown().await;
    let grpc_result = owner.shutdown().await;
    http_result.and(grpc_result)
}

fn restore_in_background(service: RuntimeService) {
    let failed = service.clone();
    let reported = service.clone();
    let worker = std::thread::Builder::new().name("mirmir-restore".to_owned()).spawn(move || {
        if let Err(error) = service.restore_active_models() {
            reported.fail_startup(error.message().to_owned());
            tracing::error!(%error, "active model restoration failed");
        }
    });
    match worker {
        Ok(handle) => drop(handle),
        Err(error) => {
            failed.fail_startup(error.to_string());
            tracing::error!(%error, "active model restoration thread failed to start");
        },
    }
}

#[cfg(unix)]
async fn shutdown_signal() -> Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _signal = terminate.recv() => {},
    }
    Ok(())
}

#[cfg(not(unix))]
async fn shutdown_signal() -> Result<()> {
    tokio::signal::ctrl_c().await?;
    Ok(())
}
