use super::{HF_TOKEN, remote, remove_operation, secret_key, set_operation, unsupported};
use crate::{
    cli::ConfigCommand,
    error::{Error, Result},
    output,
    rpc::{Client, proto},
};

pub(super) async fn attached(client: &mut Client, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Show => {
            let config =
                client.get_configuration(proto::GetConfigurationRequest {}).await?.into_inner();
            output::json(&config)
        },
        ConfigCommand::Validate => {
            // The server validates its own store when producing this snapshot.
            client.get_configuration(proto::GetConfigurationRequest {}).await?;
            output::line("server configuration is valid")
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
            remote(client, set_operation(&key, value)).await
        },
        ConfigCommand::Remove { key } => remote(client, remove_operation(&key)?).await,
        ConfigCommand::Test { key } if key == HF_TOKEN => {
            remote(
                client,
                proto::update_configuration_request::Operation::TestHfToken(proto::TestHfToken {}),
            )
            .await
        },
        ConfigCommand::Test { key } => Err(unsupported("test", &key)),
        ConfigCommand::Init | ConfigCommand::Edit => {
            Err(Error::Config("attached configuration must be managed through RPC".to_owned()))
        },
    }
}
