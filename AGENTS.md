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
- Keep `benchmarks` limited to the current index and table-only model/platform
  results. Raw harness output, historical matrices, diagnostics, and
  performance reasoning belong to the Workmir metarepository.

## Engineering rules

- Rust source files must never exceed 250 lines.
- Split modules only as `module/mod.rs` plus focused files such as
  `module/feature.rs`. Never flatten a child into a sibling such as
  `module_feature.rs`.
- Apply the same nesting to tests: use `module/tests.rs` or
  `module/tests/feature.rs`, never sibling files such as `module_tests.rs` or
  `feature_tests.rs` when they test an existing module.
- Use `thiserror`, `From` conversions, and `?`; `map_err` is a last resort at a
  foreign boundary that requires additional context.
- Do not introduce PyTorch or a Python runtime dependency.
- Keep nightly rustfmt and strict clippy clean for all targets and features.
- Keep OpenAI API routes grouped under a shared `/v1` router.
- Keep persistent paths and environment names under the `mirmir` prefix.
- Keep domain states, stages, kinds, units, outcomes, and error codes typed
  through application layers. Convert them to stable strings only at protocol,
  serialization, persistence, logging, or UI boundaries. Never infer state from
  display strings or duplicate enum-to-string mappings across consumers.
- Typed models must make invalid states unrepresentable. Do not replace related
  strings with parallel enums, booleans, and optional fields that can contradict
  one another. Wrapper enums must add domain semantics, not merely rename values.

The manifest uses a versioned crates.io release of `libmir`; do not add a Git or
relative path dependency here.

## Generation stream cancellation

- SSE adapters must observe downstream closure while awaiting the next source
  event, not only when sending a chunk. Application generation streams watch
  receiver closure independently of output and signal their cancellation token.
  Scope that watcher to producer completion so it cannot retain a sender and
  block EOF or affect an unrelated request.
- Workmir's `2026-09-08-http-cancellation` records the idle-disconnect regression
  and a real Qwen HTTP request cancelled before output, with an 8.56 s delay
  in that old build. The later `2026-09-08-prefill-cancellation` implements
  text cancellation at Metal graph boundaries and validates both model APIs;
  `2026-09-09-http-prefill-progress` now also passes HTTP disconnect after
  positive Qwen prefill progress, no output and clean unload. Its 0.431 s
  observation is not a cancellation latency benchmark.
- Default HTTP rendering enables Qwen thinking. The 24-string
  registry test exceeded 1024 tokens with reasoning only in multiple requests;
  no-thinking backend smokes do not qualify that path. Preserve the failures,
  define reasoning control explicitly, and pass a sequential quality gate
  before repeating concurrent cancellation/refill or unchanged MLX-LM timing.

- `GenerationRequest::reasoning` and HTTP `reasoning` select request-scoped
  `ModelDefault/Enabled/Disabled` template control. Preserve omitted defaults;
  do not silently ignore unsupported explicit modes or non-null
  `reasoning_budget`/`reasoning_effort`. The two completion-limit aliases count
  all output channels, not a separate reasoning allocation.
- Workmir's `2026-09-08-qwen-reasoning-control` passes one explicit disabled-mode
  HTTP answer (24 values, 89/1024 tokens, stop/DONE, clean unload). It does not
  close default-thinking quality or concurrent HTTP gates. Next cover bounded
  concurrent cancellation/refill in that declared mode before timing work.

- Workmir's `2026-09-09-qwen-http-concurrency` qualifies three complete HTTP
  disabled-thinking answers, cancel at 16 IDs and successful refill/clean unload.
  The refill overlapped HTTP lifetimes but waited until existing decode ended;
  do not claim dynamic joining of the active GPU group. A 7-token cached
  continuation waited about one second under the existing routed admission
  policy; any latency candidate needs survivor-interference and lifecycle gates.

- `runtime.cached_prefill_policy` is typed and defaults to backend behavior;
  `interleave_one_block` is an explicit latency tradeoff, reset with `auto`.
  Do not silently enable it: Qwen ABBA reduced refill TTFT to ~47 ms but slowed
  survivor decode by ~22%, failing the 15% default-promotion gate. Final opt-in
  HTTP passes semantic/cancel/unload checks. See Workmir's cached-refill report.


## Metal reservation configuration

- On macOS, runtime.metal_decode_reservation maps the public libmir enum directly
  into RuntimeConfig.metal.cache.decode_reservation. Values are one_page and
  generation_budget; auto removes the override, preserving the default OnePage.
  Keep the option unavailable on other platforms rather than silently ignoring it.
- Configuration presentation must expose the value source and restart requirement.
  CLI/TUI/server edits share Store validation before write. Runtime-field reset
  must handle ordinary and inline TOML tables; use table-like removal so auto does
  not silently leave an explicit value behind. Preserve comments and unrelated keys.
- Workmir's2026-09-09-reservation-config verifies24 config tests and10 isolated CLI
  calls. No user configuration change, model run, default promotion or MLX-LM rerun.

- Workmir's `2026-09-09-generation-budget-http` now qualifies the explicit
  GenerationBudget setting through HTTP: Qwen 6/6 answers (89 IDs), GPT-OSS 9/9
  (36/46/37 IDs), C3 cancellation at 16 IDs/refill, exact repeat IDs, prefix reuse
  and clean unload/server exit. Qwen reasoning is disabled; GPT-OSS uses default
  Harmony reasoning with separate final content. All requests have limit 256 and
  stop earlier. No product defect, user config change or default promotion.
  Logical cache counters do not prove physical arena bounds or pressure reclaim.
  Refill overlaps HTTP lifetimes but does not qualify joining active GPU decode.
  Do not repeat unchanged MLX-LM or failed thermal-readiness runs based on this gate.


## HTTP performance measurement

- Workmir's `2026-09-10-qwen-http-benchmark` stops before any GenerationBudget
  timing: six corrected OnePage C3 cold/cached observations have 3204 exact IDs
  and clean unloads, but fail TTFT/decode/frequency stability. Do not present its
  raw times as a qualified comparison or repeat the unchanged protocol.
- In serve mode, a global RUST_LOG=warn retains explicit default info targets.
  Quiet external performance harnesses must override mirmir/libmir/metal/cuda/
  tower_http targets explicitly and verify the effective log before model load.
  This behavior is intentional; do not change product logging solely for a harness.
- Cold/cached HTTP TTFT includes scheduling and delivery; short cached prefill
  cannot inherit the longer cold interval's GPU frequency. Process RSS and logical
  KV counters are not physical Metal arena measurements. Keep these limits explicit.

- Workmir's `2026-09-10-prefill-admission-age` validates the shared libmir
  queue-age deadline fix through the rebuilt application: Qwen 6/6 and GPT-OSS
  9/9 correct answers, preserved C3 cohorts, cancel/refill/reuse and identical
  completed token sequences to the preceding build. Both unload and exit cleanly.
  The measured 30–40 ms saving is a host-only collector test, not HTTP speedup.

- Workmir's `2026-09-10-admission-cancellation` verifies prefill-collector
  disconnects on Qwen and GPT-OSS: a logged cancellation before any new cohort,
  no output, restored logical block counts, then correct generation/reuse and
  clean unload. Positive prompt-token telemetry appears too late to target this
  collector; the first late probe is archived. Use the cancellation debug event
  as execution evidence. The enlarged diagnostic quiet window is not a default.
