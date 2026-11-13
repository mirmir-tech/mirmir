use clap::Parser;

use super::with_port;
use crate::{
    cli::{Cli, Command},
    instance::Instance,
    remote::Trust,
};

#[test]
fn accepts_selection_before_and_after_subcommands() {
    for arguments in [
        vec!["mirmir", "--system", "status"],
        vec!["mirmir", "status", "--system"],
        vec!["mirmir", "config", "show", "--system"],
    ] {
        let cli = Cli::try_parse_from(arguments).expect("global instance option");
        assert!(matches!(Instance::try_from(cli.instance).expect("instance"), Instance::System));
    }
    let cli = Cli::try_parse_from(["mirmir", "--socket", "/tmp/service/runtime.sock"])
        .expect("attached TUI");
    assert!(cli.command.is_none());
    assert!(matches!(
        Instance::try_from(cli.instance).expect("instance"),
        Instance::Socket(_)
    ));
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

#[test]
fn selects_a_remote_server_with_a_pinned_certificate() {
    let pin = "AB".repeat(32);
    let cli = Cli::try_parse_from([
        "mirmir",
        "--remote",
        "ziemniak.local",
        "--remote-key",
        "mmk_0011223344556677_secret",
        "--remote-fingerprint",
        pin.as_str(),
        "status",
    ])
    .expect("remote selection");
    let Ok(Instance::Remote(target)) = Instance::try_from(cli.instance) else {
        panic!("expected a remote instance");
    };
    assert_eq!(target.address, "ziemniak.local:7443");
    assert!(matches!(target.trust, Trust::Pinned(_)));
}

#[test]
fn a_remote_server_requires_a_key_and_excludes_local_selection() {
    let cli = Cli::try_parse_from(["mirmir", "--remote", "host:9000", "status"]).expect("parse");
    if std::env::var_os("MIRMIR_REMOTE_KEY").is_none() {
        assert!(Instance::try_from(cli.instance).is_err());
    }
    assert!(Cli::try_parse_from(["mirmir", "--system", "--remote", "host"]).is_err());
    assert!(Cli::try_parse_from(["mirmir", "--socket", "/tmp/s", "--remote", "host"]).is_err());
}

#[test]
fn adds_the_default_remote_port() {
    assert_eq!(with_port("host"), "host:7443");
    assert_eq!(with_port("host:9000"), "host:9000");
    assert_eq!(with_port("10.0.0.4"), "10.0.0.4:7443");
    assert_eq!(with_port("::1"), "[::1]:7443");
    assert_eq!(with_port("[::1]:9000"), "[::1]:9000");
}
