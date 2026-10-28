# Administering a system instance

The service can run as a non-login `mirmir` account while operators use their own
accounts. The CLI and TUI connect to the service's existing Unix gRPC socket.
There is no separate operator runtime or copy of the service's configuration.

## Selecting the instance

```sh
mirmir --system
mirmir --system status
mirmir --system config show
mirmir --system config validate
mirmir --system model list
```

`--system` selects `/run/mirmir/mirmir.sock`. `--socket /path/to/runtime.sock`
selects another instance. These global options also work after a subcommand;
they cannot be combined. With either option, a client only connects: missing
sockets, denied access, and incompatible protocols are errors. The client never
starts a replacement runtime or falls back to personal configuration. Closing
the TUI leaves the service running. Its header shows the instance and socket.

Attached clients do not require `HOME`, read local runtime configuration, or
load `.env` files from the current directory. `config show/set/remove/test` use
RPC; `config validate` asks the service for a validated configuration snapshot.
`config init/edit` are rejected because they operate directly on local files.
An omitted `prompt --model` uses the selected server's configured default model.
Without an instance option, the existing personal mode remains available.

## Existing Linux service

Build the Linux binary for the staging server and install it there first. Keep
the service's existing configuration, state paths, environment, and GPU settings. `--system serve`
changes the socket location and grants its OS group access; it does not relocate
configuration, model storage, or secrets.

Create `mirmir-admin` and add the operator account using the host's account
management tools. Open a new login session so the membership takes effect.
For an existing `mirmir.service`, add this systemd drop-in, adapting the binary
path and preserving the arguments from the current `ExecStart`:

```ini
[Service]
User=mirmir
Group=mirmir-admin
RuntimeDirectory=mirmir
RuntimeDirectoryMode=0750
ExecStart=
ExecStart=/usr/local/bin/mirmir --system serve
```

Reload systemd's configuration and restart the staging service. systemd creates
`/run/mirmir` with the service user and group; MiRMiR creates a `0660` socket and
a private instance lock. The service's configuration/state directories stay
`0700`; stored secrets stay `0600`. Members of `mirmir-admin` have full RPC
administration rights, including configuration changes and model operations.

For a custom socket, start the service with
`mirmir --socket /run/mirmir-staging/runtime.sock serve --shared-socket`.
Provision its parent directory as `mirmir:mirmir-admin`, mode `0750`, and run
the service with that effective group. MiRMiR sets new dedicated directories to
`0750`, but preserves permissions on existing external parents. The socket's
group follows normal OS creation rules, including a parent's setgid bit.
Without `--shared-socket`, a custom socket is owner-only (`0600`).

The systemd settings follow the upstream documentation for
[service users and runtime directories](https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml)
and [replacing ExecStart](https://github.com/systemd/systemd/blob/main/man/systemd.service.xml).

## Acceptance test on staging

1. Check `id` as the operator, then inspect `/run/mirmir` and its socket. Expect
   owner/group `mirmir:mirmir-admin`, directory `0750`, socket `0660`.
2. Run `mirmir --system status`, `mirmir --system config show`, and
   `mirmir --system model list`. Compare models, configuration paths, and live
   telemetry with the service. The configuration
   path must belong to the service, rather than the operator's home directory.
3. Open `mirmir --system`, verify the `SYSTEM` header and socket address, and
   close it. Confirm the service PID is unchanged and `status` still works.
4. Load a small available model on staging, send one request, and verify activity
   and generation statistics appear in the operator TUI and `status` output.
5. Record `server.max_concurrency`, change it through `--system config set`,
   confirm the service's file changed and the operator's files did not, then
   restore the original value. Respect the reported restart requirement.
6. Repeat `mirmir --system status` from an account outside `mirmir-admin`. Expect a permission
   error, a nonzero exit status, and no new personal runtime or files.
7. Stop the staging service. Repeat status, configuration commands, and opening
   the TUI with `--system`. Each must fail without starting a replacement runtime
   or modifying the operator's configuration. Start the staging service again.
8. Restart the service and repeat operator access to verify that socket group
   permissions survive recreation. Verify personal mode separately with
   isolated `MIRMIR_CONFIG_HOME`, `MIRMIR_STATE_HOME`, and `XDG_RUNTIME_DIR`.

Automated local checks:

```sh
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features
cargo fmt --check
```

The `instance-cli` integration tests run an isolated real service without HTTP
or model loading. They verify status, configuration changes, model listing,
default-model selection, failure without fallback, and clients with no `HOME`
and invalid local configuration/`.env`. Unit tests cover permissions, ownership
of the connection, configuration isolation, and instance selection. Cross-user
OS access and systemd lifecycle are covered by the staging steps above.
