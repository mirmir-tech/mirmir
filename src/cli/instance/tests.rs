use clap::Parser;

use crate::{
    cli::{Cli, Command},
    instance::Instance,
};

#[test]
fn accepts_selection_before_and_after_subcommands() {
    for arguments in [
        vec!["mirmir", "--system", "status"],
        vec!["mirmir", "status", "--system"],
        vec!["mirmir", "config", "show", "--system"],
    ] {
        let cli = Cli::try_parse_from(arguments).expect("global instance option");
        assert!(matches!(Instance::from(cli.instance), Instance::System));
    }
    let cli = Cli::try_parse_from(["mirmir", "--socket", "/tmp/service/runtime.sock"])
        .expect("attached TUI");
    assert!(cli.command.is_none());
    assert!(matches!(Instance::from(cli.instance), Instance::Socket(_)));
}

#[test]
fn rejects_conflicting_selection() {
    assert!(Cli::try_parse_from(["mirmir", "--system", "--socket", "/tmp/socket"]).is_err());
}

#[test]
fn parses_group_access_for_custom_server() {
    let cli = Cli::try_parse_from([
        "mirmir",
        "--socket",
        "/tmp/service/runtime.sock",
        "serve",
        "--shared-socket",
    ])
    .expect("shared custom server");
    assert!(matches!(cli.command, Some(Command::Serve(args)) if args.shared_socket));
}
