# Remote access

A server can offer its `Runtime` gRPC service to other computers on a second
listener. That listener speaks TLS and requires an API key in every call. The
local Unix socket is unchanged: local clients need no key. Mirmir for Mac, the
CLI and the TUI all use the same service.

## Enable it on the server

```sh
mirmir remote enable                 # listens on 0.0.0.0:7443
mirmir remote enable --bind 10.0.0.4:7443
```

For a system service, add `--system` and run the command as a member of the
service's group. `enable` starts the listener immediately and keeps
`server.remote_bind` in `config.toml`, so it also starts with `mirmir serve`.
When no key exists yet, it creates a `control` key and prints it **once**. It
also prints the certificate fingerprint.

`server.remote_bind` changes only through `mirmir remote enable` and
`mirmir remote disable`, which start and stop the listener in the same step;
`mirmir config set server.remote_bind` is refused. Enabling another address
binds it before the running listener stops, so a failure leaves the running
listener and its connections in place. Enabling the running address and
certificate again changes nothing.

Only `mirmir serve` opens the listener. A server that a CLI command or the TUI
starts because none is running never does: `mirmir remote status` then reports
remote access as configured but not listening.

```sh
mirmir remote status                 # listener, fingerprint, keys, recent clients
mirmir remote disable                # stop listening; keys are kept
```

Open the port in the host firewall for the networks that should reach it.

## Keys and scopes

```sh
mirmir remote key create --name studio-mac --scope use
mirmir remote key create --name ops --scope control
mirmir remote key list
mirmir remote key revoke 3f9c0d6e1a2b4c5d
```

| Scope | Allows |
| --- | --- |
| `use` | Chat (`Generate`), `Embed`, `Rerank`, model lists and inspection, catalog search, telemetry, activity and the redacted configuration |
| `control` | Everything in `use`, plus `LoadModel` and `UnloadModel`, downloading and removing models, model generation defaults, model and runtime settings, testing the Hugging Face token, and cancelling operations |

`Generate`, `Embed` and `Rerank` run on the model they name, as on every
frontend: with a `use` key they load a known model that is not resident yet,
which may unload another one to make room. Loading or unloading a model on its
own needs `control`.

A remote key names a model by a configured key, by a model the server has
loaded, or by a path the server lists itself (`ListLocalModels`): a configured
model's directory or a cached snapshot. Any other path is refused like an
unknown model, so a key cannot make the server open other directories or tell
which paths exist. Local clients may name any path.

Remote access itself is managed only over the local socket. No key, whatever
its scope, can create or revoke keys or change the listener. Server settings
(every `server.*` key: listeners, TLS paths, HTTP exposure and limits) and the
stored secrets (the Hugging Face token and the HTTP API key) are local-only
too: a `control` key gets `PERMISSION_DENIED`. For remote keys the
configuration omits `raw_toml`, `config_path` and `secrets_path` and shows
server settings as not editable. One table in `src/remote/scope.rs` assigns
each method its scope, and `remote::reserved` there names the local-only
settings; a method without an entry is refused remotely, and a test fails when
a protocol method has none.

A key reads `mmk_<id>_<secret>`. The id is public and appears in lists. The
server stores only the SHA-256 of the secret and compares it in constant time.
Keys are checked when each call starts. A revoked key is refused on its next
request; a stream it opened before, such as a running generation or
`WatchActivity`, continues until it ends or the client closes it. Keys created
while the server runs apply immediately.

Remote keys are separate from `server.api_key`, the key of the
OpenAI-compatible HTTP API, and do not work in its place. A shared key would
give every HTTP client full control of the server.

## Certificates

Without configuration the server generates a self-signed certificate once in
`<state>/remote/tls/` and keeps it across restarts. Clients pin it by the
SHA-256 of its DER form, shown as colon-separated hex. Compare the value with
`mirmir remote status` on the server before trusting it. Mirmir for Mac pins
the fingerprint the first time it connects and keeps it. The CLI and TUI keep
nothing: pass `--remote-fingerprint` or set `MIRMIR_REMOTE_FINGERPRINT` on
every run, since without it they verify against the system trust store, which
refuses a self-signed certificate. A later certificate change is refused by
pinned clients.

To use a CA-issued certificate:

```sh
mirmir config set server.remote_tls_cert /etc/mirmir/remote/fullchain.pem
mirmir config set server.remote_tls_key /etc/mirmir/remote/privkey.pem
mirmir remote enable
```

Clients then verify it against the system trust store and need no fingerprint.

Each command saves one key. The listener needs both: while only one is set,
`mirmir config show` reports the other as `missing`, `mirmir remote enable`
fails with an error naming it, and a server start keeps the listener off and
shows that error in `mirmir remote status`. `mirmir remote enable` restarts a
running listener with the new certificate; when the new one cannot serve, the
previous listener starts again and the command reports the error. Run these
commands on the server: remote keys cannot change server settings. To return
to the self-signed certificate, reset both keys and enable again:

```sh
mirmir config set server.remote_tls_cert auto
mirmir config set server.remote_tls_key auto
mirmir remote enable
```

## Connect from another computer

```sh
mirmir remote fingerprint ziemniak.local        # what the server presents
export MIRMIR_REMOTE_KEY=mmk_…
mirmir --remote ziemniak.local \
       --remote-fingerprint 'AB:CD:…' status
mirmir --remote ziemniak.local --remote-fingerprint 'AB:CD:…' model list
mirmir --remote ziemniak.local --remote-fingerprint 'AB:CD:…'   # TUI
```

`--remote` takes `HOST[:PORT]`; the port defaults to 7443, and the
`grpcs://host:port` form the TUI shows is accepted too. The CLI does not
remember a fingerprint: give `--remote-fingerprint` (or
`MIRMIR_REMOTE_FINGERPRINT`) on every run, or the certificate must chain to a
system CA. Like `--system`, a remote client only attaches: it never starts or
configures a local runtime. The TUI header shows `REMOTE` and `grpcs://host:port`.

`mirmir remote fingerprint` only reads the certificate. It sends no request and
no key.

Reaching the server (TCP, the TLS handshake and the first Health call) must
finish within 10 seconds; otherwise the command fails and names the address.
Calls on an established connection, such as generation, loading or downloads,
keep running as long as they need.

## Protection and limits

- TLS 1.2 or 1.3 through rustls; the handshake proves the server holds the
  pinned certificate's private key.
- A missing or unknown key returns `UNAUTHENTICATED`; a key whose scope is too
  narrow, or a local-only method, returns `PERMISSION_DENIED`.
- After 20 failed attempts within a minute, further failed attempts from the
  same IPv4 address or IPv6 /64 are refused for the rest of the minute
  (`RESOURCE_EXHAUSTED`). Clients behind one NAT, or hosts in one /64, share
  that budget, but a valid key is never refused by it.
- Rejections are logged with the peer address and method, never the key.
- Both the local socket and the TLS listener accept requests up to 32 MiB, room
  for an image of up to 20 MiB with its conversation; a larger image is refused
  with `INVALID_ARGUMENT`.
- The server lists clients seen in the last 15 minutes with the key name,
  scope, peer address and request count, at most 1024 of them. Each key's last
  use is kept in memory and resets when the server restarts.
- The listener does not limit which networks connect; use the bind address and
  the firewall for that.
