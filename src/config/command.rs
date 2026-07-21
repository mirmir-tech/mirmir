use std::{fs, process::Command};

use crate::{
    catalog::Catalog,
    cli::ConfigCommand,
    config::{Paths, Store},
    error::{Error, Result},
    output,
    rpc::{self, proto},
};

const HF_TOKEN: &str = "hugging_face.token";
const HTTP_API_KEY: &str = "server.api_key";

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
            let value = value.map_or_else(
                || {
                    if secret_key(&key) {
                        output::read_secret()
                    } else {
                        Err(Error::Config(format!("value is required for `{key}`")))
                    }
                },
                Ok,
            )?;
            if let Some(client) = client.as_mut() {
                remote(client, set_operation(&key, value)).await
            } else {
                set_local(store, &key, &value)?;
                output::line(format!("saved {key}"))
            }
        },
        ConfigCommand::Remove { key } => {
            if let Some(client) = client.as_mut() {
                remote(client, remove_operation(&key)?).await
            } else {
                remove_local(store, &key)?;
                output::line(format!("removed {key}"))
            }
        },
        ConfigCommand::Test { key } => test_value(store, client.as_mut(), &key).await,
    }
}

fn secret_key(key: &str) -> bool {
    matches!(key, HF_TOKEN | HTTP_API_KEY)
}

fn set_operation(key: &str, value: String) -> proto::update_configuration_request::Operation {
    use proto::update_configuration_request::Operation;
    match key {
        HF_TOKEN => Operation::SetHfToken(proto::SetHfToken { token: value }),
        HTTP_API_KEY => Operation::SetHttpApiKey(proto::SetHttpApiKey { key: value }),
        _ => Operation::SetValue(proto::SetConfigurationValue { key: key.to_owned(), value }),
    }
}

fn set_local(store: &Store, key: &str, value: &str) -> Result<()> {
    match key {
        HF_TOKEN => store.set_hf_token(value.trim()),
        HTTP_API_KEY => store.set_http_api_key(value.trim()),
        _ => store.set_config_value(key, value),
    }
}

fn remove_operation(key: &str) -> Result<proto::update_configuration_request::Operation> {
    use proto::update_configuration_request::Operation;
    match key {
        HF_TOKEN => Ok(Operation::RemoveHfToken(proto::RemoveHfToken {})),
        HTTP_API_KEY => Ok(Operation::RemoveHttpApiKey(proto::RemoveHttpApiKey {})),
        _ => Err(unsupported("remove", key)),
    }
}

fn remove_local(store: &Store, key: &str) -> Result<()> {
    match key {
        HF_TOKEN => store.remove_hf_token(),
        HTTP_API_KEY => store.remove_http_api_key(),
        _ => Err(unsupported("remove", key)),
    }
}

fn unsupported(operation: &str, key: &str) -> Error {
    Error::Config(format!("`config {operation}` is not supported for `{key}`"))
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

async fn test_value(store: &Store, client: Option<&mut rpc::Client>, key: &str) -> Result<()> {
    if key != HF_TOKEN {
        return Err(unsupported("test", key));
    }
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
