# Mirmir Agent Notes

This repository owns the single `mirmir` application binary. Model execution
belongs to the public `libmir` API; model-agnostic accelerator work remains
encapsulated by its backend dependencies.

## Boundaries

- `src/config`: typed application, transport, model, and secret configuration.
- `src/daemon` and `src/rpc`: process lifecycle and private gRPC over UDS.
- `src/http`: optional OpenAI-compatible HTTP adapter.
- `src/catalog`: Hugging Face discovery, fit estimation, pull, and remove.
- `src/tui`: Ratatui dashboard using only the gRPC client contract.
- Depend only on public `libmir` APIs for model and runtime behavior.
- Do not add direct dependencies on `metal`, `mirtal`, `mircuda`, or private
  runtime crates.
- The legacy `cli`, `config`, `server`, and `web` directories are excluded from
  the active workspace and must not become runtime dependencies.
- Keep environment and CLI precedence in the application layer. Library
  defaults and typed runtime configuration remain in `libmir`.
- Keep tracing subscriber setup here; libraries emit events but do not choose
  presentation or collectors.

## Engineering rules

- Rust source files must never exceed 250 lines.
- Split modules as `module/mod.rs` plus focused feature files.
- Use `thiserror`, `From` conversions, and `?`; `map_err` is a last resort at a
  foreign boundary that requires additional context.
- Do not introduce PyTorch or a Python runtime dependency.
- Keep nightly rustfmt and strict clippy clean for all targets and features.
- Keep OpenAI API routes grouped under a shared `/v1` router.
- Keep persistent paths and environment names under the `mirmir` prefix.

The manifest uses a versioned crates.io release of `libmir`; do not add a Git or
relative path dependency here.
