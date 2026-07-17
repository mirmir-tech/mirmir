use futures_util::StreamExt;

use crate::{
    cli::ModelCommand,
    config::{AppConfig, Paths},
    daemon,
    error::{Error, Result},
    output,
    rpc::{self, proto},
};

pub async fn run(paths: Paths, config: AppConfig, command: ModelCommand) -> Result<()> {
    let mut connection = daemon::connect_or_start(&paths, &config).await?;
    let result = execute(&mut connection.client, command).await;
    result.and(connection.shutdown().await)
}

async fn execute(client: &mut rpc::Client, command: ModelCommand) -> Result<()> {
    match command {
        ModelCommand::List => {
            let response =
                client.list_local_models(proto::ListLocalModelsRequest {}).await?.into_inner();
            output::json(&response)
        },
        ModelCommand::Inspect { selector } => {
            let response = client
                .inspect_model(proto::InspectModelRequest { selector })
                .await?
                .into_inner();
            output::json(&response)
        },
        ModelCommand::Search { query, limit } => {
            let response = client
                .search_models(proto::SearchModelsRequest { query, limit, cursor: None })
                .await?
                .into_inner();
            output::json(&response)
        },
        ModelCommand::Pull { repo_id, revision } => {
            let mut stream = client
                .pull_model(proto::PullModelRequest { repo_id, revision })
                .await?
                .into_inner();
            let mut path = None;
            while let Some(event) = stream.next().await {
                let event = event?;
                if let Some(total) = event.total_bytes.filter(|total| *total > 0) {
                    let percent = (event.downloaded_bytes.saturating_mul(100) / total).min(100);
                    output::diagnostic(format!(
                        "{} {percent:>3}% — {}",
                        event.phase, event.message
                    ))?;
                } else {
                    output::diagnostic(format!("{} — {}", event.phase, event.message))?;
                }
                path = event.path.or(path);
            }
            output::line(path.map_or_else(
                || "model downloaded".to_owned(),
                |path| format!("model available at {path}"),
            ))
        },
        ModelCommand::Remove { repo_id } => {
            let response =
                client.remove_model(proto::RemoveModelRequest { repo_id }).await?.into_inner();
            if response.removed {
                output::line(format!("model removed; freed {} bytes", response.freed_bytes))
            } else {
                output::line("model was not downloaded")
            }
        },
        ModelCommand::Load { selector, force } => {
            let mut stream = client
                .load_model(proto::LoadModelRequest { selector, force, ..Default::default() })
                .await?
                .into_inner();
            let mut loaded = None;
            while let Some(event) = stream.next().await {
                let event = event?;
                if let Some(total) = event.total.filter(|total| *total > 0) {
                    let percent = (event.current.saturating_mul(100) / total).min(100);
                    output::diagnostic(format!(
                        "{} {percent:>3}% — {}",
                        event.phase, event.detail
                    ))?;
                } else {
                    output::diagnostic(format!("{} — {}", event.phase, event.detail))?;
                }
                loaded = event.model.or(loaded);
            }
            let model = loaded.ok_or_else(|| {
                Error::Config("model load stream ended without a ready model".to_owned())
            })?;
            output::line(format!("loaded {} from {}", model.id, model.path))
        },
        ModelCommand::Unload { selector } => {
            let response =
                client.unload_model(proto::UnloadModelRequest { selector }).await?.into_inner();
            if response.unloaded {
                output::line("model unloaded")
            } else {
                output::line("model was not loaded")
            }
        },
    }
}
