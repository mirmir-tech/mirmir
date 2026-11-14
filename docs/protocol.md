# Runtime gRPC protocol changes

`Health.protocol_version` identifies the `mirmir.v1.Runtime` contract in
`proto/mirmir.proto`. Clients must refuse a server whose version differs from
their own.

## Protocol 2

### Remote access management

`GetRemoteAccess`, `SetRemoteAccess`, `CreateRemoteKey` and `RevokeRemoteKey`
manage the TLS listener and its API keys. They are served only over the local
socket; over TLS they return `PERMISSION_DENIED` for every key. Scopes are the
typed `RemoteKeyScope` (`USE`, `CONTROL`). `CreateRemoteKeyResponse.secret` is
the only time a key's value is sent. Remote calls carry
`authorization: Bearer mmk_…` metadata. See [remote access](remote-access.md).

`RemoteAccessStatus.bind` is empty when remote access is off. Otherwise at most
one of `listener` and `error` is set; neither means the listener is configured
but not running, as on a server a command started on demand. Invalid input to
these RPCs (a key name, an address) returns `INVALID_ARGUMENT`, a server that
is shutting down `UNAVAILABLE`, and a listener that cannot start
`FAILED_PRECONDITION`. `server.remote_bind` changes only through
`SetRemoteAccess`; `UpdateConfiguration` refuses it with `INVALID_ARGUMENT`.

Over TLS, `UpdateConfiguration` refuses every `server.*` value and every secret
operation (`SetHfToken`, `RemoveHfToken`, `SetHttpApiKey`, `RemoveHttpApiKey`)
with `PERMISSION_DENIED`, whatever the key's scope. `ConfigurationSnapshot`
then has empty `raw_toml`, `config_path` and `secrets_path`, and `server.*`
values are not `editable`. Model selectors from a remote key must be empty (the
default model), a loaded model, a configured key or a path `ListLocalModels`
lists; anything else returns the `INVALID_ARGUMENT` of an unknown model.

### Telemetry memory pools are typed and explicit

Protocol 1 sent one memory figure with a free-form `memory_source` label, and
the two telemetry RPCs disagreed. `Telemetry` reported the accelerator budget
(the Metal working set, labelled `Metal unified memory`). `TelemetryHistory`
reported host RAM from the operating system (`macOS unified system memory`).
On a 64 GB M3 Max the live value was 55.7 GB and the history value 68.7 GB.

Protocol 2 removes the label and reports each memory pool in its own typed
field:

| RPC | Field | Meaning |
| --- | --- | --- |
| `Telemetry` | `TelemetrySnapshot.host_memory` | Host RAM from the operating system. |
| `Telemetry` | `TelemetrySnapshot.accelerator_memory` | Accelerator backend budget, with an `AcceleratorMemoryPool`. |
| `TelemetryHistory` | `TelemetryHistorySample.host_memory` | Host RAM, sampled by the same code as `TelemetrySnapshot.host_memory`. |

- `HostMemory { total_bytes, available_bytes }` is absent only when the
  operating system reports no total.
- `AcceleratorMemory { pool, total_bytes, available_bytes }` is absent until
  the backend is initialized, or when it reports no budget. The `pool` value
  comes from the compiled backend and its unified-memory flag, never from a
  label:
  - `ACCELERATOR_MEMORY_POOL_METAL_WORKING_SET`: Metal's recommended working
    set within unified memory (macOS).
  - `ACCELERATOR_MEMORY_POOL_CUDA_DEVICE`: dedicated CUDA device memory.
  - `ACCELERATOR_MEMORY_POOL_CUDA_UNIFIED`: integrated CUDA memory bounded by
    the shared host pool.
- History has no accelerator memory. The history recorder is host-only by
  design: it must never hold the runtime or an accelerator handle. To compare
  live and historical values, use `host_memory` from both RPCs.
- Removed fields are reserved: `TelemetrySnapshot` fields 14–16
  (`host_total_memory_bytes`, `host_available_memory_bytes`, `memory_source`)
  and `TelemetryHistorySample` fields 8–10 (`memory_total_bytes`,
  `memory_available_bytes`, `memory_source`).
- The persisted history file is now schema 2. A schema 1 file is reported as
  unreadable once and replaced on the next flush. History is a 15-minute ring,
  so nothing else is lost.
- The HTTP dashboard overview keeps its JSON shape. Its
  `host_*_memory_bytes` fields now always mean host RAM, which matches the
  dashboard's "Host memory" label and its history chart.

### Local model residency uses canonical paths

`ListActiveModels` and model loading resolve a path selector to its canonical
path. `ListLocalModels` listed discovered Hub cache snapshots under the path it
had walked. When `$MIRMIR_STATE_HOME/hf-hub` was a symlink, a restored model
showed as `available` under the symlink path, while `ListActiveModels`
reported it as active under the resolved path.

`ListLocalModels` now canonicalizes discovered snapshot paths before it matches
residency and loading state. It also canonicalizes them before removing
duplicates of configured models and before checking that a model is managed.
A discovered snapshot's `path` and `selector` are therefore the canonical path
that `ListActiveModels` returns. The message shapes do not change.
