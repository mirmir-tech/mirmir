use std::{fs, process::Command};

use crate::{
    catalog::Catalog,
    cli::ConfigCommand,
    config::{Paths, Store},
    error::{Error, Result},
    output,
    rpc::{self, proto},
};

pub async fn run(paths: &Paths, store: &Store, command: ConfigCommand) -> Result<()> {
    let mut client = optional_client(paths).await?;
    match command {
        ConfigCommand::Init => {
            store.initialize()?;
            output::line(format!("initialized {}", store.paths().config_file.display()))
        },
        ConfigCommand::Show => show(store, client.as_mut()).await,
        ConfigCommand::Edit => {
            if client.is_some() {
                return Err(Error::Config(
                    "stop the running mirmir server before editing config.toml".to_owned(),
                ));
            }
            edit(store)
        },
        ConfigCommand::Validate => {
            drop(store.load()?);
            output::line("configuration is valid")
        },
        ConfigCommand::Set { key, value } => {
            if let Some(client) = client.as_mut() {
                remote(
                    client,
                    proto::update_configuration_request::Operation::SetValue(
                        proto::SetConfigurationValue { key, value },
                    ),
                )
                .await
            } else {
                store.set_config_value(&key, &value)?;
                output::line(format!("saved {key}"))
            }
        },
        ConfigCommand::SetHfToken { token } => {
            let token = token.map_or_else(output::read_secret, Ok)?;
            if let Some(client) = client.as_mut() {
                remote(
                    client,
                    proto::update_configuration_request::Operation::SetHfToken(proto::SetHfToken {
                        token,
                    }),
                )
                .await
            } else {
                store.set_hf_token(token.trim())?;
                output::line("Hugging Face token saved")
            }
        },
        ConfigCommand::RemoveHfToken => {
            if let Some(client) = client.as_mut() {
                remote(
                    client,
                    proto::update_configuration_request::Operation::RemoveHfToken(
                        proto::RemoveHfToken {},
                    ),
                )
                .await
            } else {
                store.remove_hf_token()?;
                output::line("Hugging Face token removed")
            }
        },
        ConfigCommand::TestHfToken => test_hf_token(store, client.as_mut()).await,
        ConfigCommand::SetHttpApiKey { key } => {
            let key = key.map_or_else(output::read_secret, Ok)?;
            if let Some(client) = client.as_mut() {
                remote(
                    client,
                    proto::update_configuration_request::Operation::SetHttpApiKey(
                        proto::SetHttpApiKey { key },
                    ),
                )
                .await
            } else {
                store.set_http_api_key(key.trim())?;
                output::line("HTTP API key saved")
            }
        },
        ConfigCommand::RemoveHttpApiKey => {
            if let Some(client) = client.as_mut() {
                remote(
                    client,
                    proto::update_configuration_request::Operation::RemoveHttpApiKey(
                        proto::RemoveHttpApiKey {},
                    ),
                )
                .await
            } else {
                store.remove_http_api_key()?;
                output::line("HTTP API key removed")
            }
        },
    }
}

fn edit(store: &Store) -> Result<()> {
    store.initialize()?;
    let path = &store.paths().config_file;
    let temporary = path.with_file_name(format!(".config.edit.{}.toml", std::process::id()));
    fs::copy(path, &temporary)?;
    let result = edit_temporary(store, &temporary);
    let cleanup = fs::remove_file(&temporary);
    result?;
    cleanup?;
    output::line(format!("saved {}", path.display()))
}

fn edit_temporary(store: &Store, temporary: &std::path::Path) -> Result<()> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned());
    let status = Command::new(&editor)
        .arg(temporary)
        .status()
        .map_err(|error| Error::Config(format!("failed to start editor `{editor}`: {error}")))?;
    if !status.success() {
        return Err(Error::Config(format!("editor `{editor}` exited with {status}")));
    }
    store.replace_config(&fs::read_to_string(temporary)?)
}

async fn optional_client(paths: &Paths) -> Result<Option<rpc::Client>> {
    match rpc::connect(&paths.socket_file).await {
        Ok(client) => Ok(Some(client)),
        Err(Error::Transport(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn show(store: &Store, client: Option<&mut rpc::Client>) -> Result<()> {
    if let Some(client) = client {
        let config =
            client.get_configuration(proto::GetConfigurationRequest {}).await?.into_inner();
        output::json(&config)
    } else {
        output::json(&store.configuration()?)
    }
}

async fn test_hf_token(store: &Store, client: Option<&mut rpc::Client>) -> Result<()> {
    if let Some(client) = client {
        remote(
            client,
            proto::update_configuration_request::Operation::TestHfToken(proto::TestHfToken {}),
        )
        .await
    } else {
        let identity = Catalog::new(store.clone()).test_hf_token().await?;
        output::line(format!("token valid for {identity}"))
    }
}

async fn remote(
    client: &mut rpc::Client,
    operation: proto::update_configuration_request::Operation,
) -> Result<()> {
    let response = client
        .update_configuration(proto::UpdateConfigurationRequest { operation: Some(operation) })
        .await?
        .into_inner();
    output::line(if response.restart_required {
        format!("{}; restart required", response.message)
    } else {
        response.message
    })
}
