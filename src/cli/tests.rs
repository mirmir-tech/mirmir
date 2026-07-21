use clap::{CommandFactory, Parser};

use super::{Cli, Command, ConfigCommand};

#[test]
fn every_command_and_argument_has_help_text() {
    assert_documented(&Cli::command());
}

fn assert_documented(command: &clap::Command) {
    assert!(
        command.get_about().is_some(),
        "missing description for `{}`",
        command.get_name()
    );
    for argument in command.get_arguments() {
        assert!(
            argument.get_help().is_some(),
            "missing description for `{} {}`",
            command.get_name(),
            argument.get_id()
        );
    }
    for nested in command.get_subcommands() {
        assert_documented(nested);
    }
}

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
