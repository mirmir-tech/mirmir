# MiRMiR

MiRMiR is a native LLM runtime, interactive dashboard, and
OpenAI-compatible server for Apple Metal and NVIDIA CUDA. It runs local models
without Python, PyTorch, or Transformers in the request path.

MiRMiR is built on [libmir](https://github.com/mirmir-tech/libmir), the native
Rust inference library that provides model loading, generation, scheduling,
K/V cache management, and hardware backend integration.

Use MiRMiR when you want one binary to download and manage models, serve
applications, chat locally, and observe inference performance directly on your
own hardware.

## Two views of the same runtime

### Terminal dashboard

![MiRMiR terminal dashboard](https://mirmir.tech/assets/screenshots/tui-dashboard-0.3.0-dc39cd5e.png)

The TUI brings model management, chat, configuration, request activity, memory,
latency, throughput, and K/V cache telemetry into the terminal.

### Web dashboard

![MiRMiR web dashboard](https://mirmir.tech/assets/screenshots/web-dashboard-0.3.0.png)

The optional web dashboard exposes the same local runtime, models, settings,
and telemetry in a browser.

## Install

```sh
cargo install mirmir
mirmir
```

The current native backends require Apple Silicon with Metal and MLX, or Linux
with a supported NVIDIA GPU and CUDA toolkit. See the
[installation guide](https://docs.mirmir.tech/getting-started/install.html) for
platform preparation.

Run the server without the dashboard:

```sh
mirmir serve
```

## What it provides

- local Hugging Face model discovery, download, inspection, and lifecycle;
- terminal and browser dashboards backed by the same runtime;
- streamed text and vision generation;
- OpenAI-compatible HTTP and SSE endpoints;
- private gRPC control over a Unix socket;
- concurrent scheduling, paged K/V cache, and prefix reuse;
- live latency, throughput, memory, cache, and request telemetry;
- native Metal and CUDA execution through `libmir`.

## HTTP reasoning control

`POST /v1/chat/completions` accepts the Mirmir extension `reasoning`:
`"model_default"` (also the omitted default), `"enabled"`, or `"disabled"`.
It selects the thinking switch for that text request without changing other
requests. For example, a Qwen request can include:

```json
{"model":"/path/to/qwen", "messages":[{"role":"user","content":"Return 2+2."}], "reasoning":"disabled", "max_completion_tokens":256, "stream":true}
```

Explicit modes require a model Jinja template using `enable_thinking` or a
supported built-in Qwen/Gemma template. Other templates, including GPT-OSS
Harmony without this switch, and image requests reject explicit modes.
`model_default` preserves existing rendering, including Qwen thinking.

`max_completion_tokens` and `max_tokens` are aliases for one total budget
covering both reasoning and final content; when both are present they must
agree. Reaching that budget does not guarantee a final answer.
Non-null `reasoning_budget` and `reasoning_effort` are rejected as unsupported.
This HTTP extension is not exposed through the CLI or private gRPC protocol.

## Cached refill latency policy

The default scheduling policy is unchanged. To opt into earlier admission of
one short cached continuation during resident decode, set:

```sh
mirmir config set runtime.cached_prefill_policy interleave_one_block
```

The setting applies when the runtime starts. Restore the backend default with
`mirmir config set runtime.cached_prefill_policy auto`. The explicit default
value is `backend_default`.

This mode trades resident decode latency for earlier refill output. In the
bounded Qwen diagnostic, refill first-token latency fell from about 1 s to
47 ms, while survivor decode took about 22% longer. It did not pass the gate
for default promotion. Admission requires a free slot, a cached queue head,
and at most one KV block of estimated work; actual replay is bounded per step.

## Metal decode page reservation

On macOS, optionally reserve available K/V pages against each request's generation
budget:

```sh
mirmir config set runtime.metal_decode_reservation generation_budget
```

The setting takes effect when the runtime starts; restart a running server after
changing it. CLI, TUI and server configuration use the same setting. Unwritten
reserved pages can be reclaimed under pressure. This does not set the output-token
limit; the request still supplies that limit.

The default remains `one_page`. Restore the backend default with:

```sh
mirmir config set runtime.metal_decode_reservation auto
```

Use `one_page` to select that policy explicitly. `auto` removes the override from
`config.toml`. This option remains opt-in: isolated decode gains have not yet
qualified whole-request prefill protection. Other platforms reject this Metal-only
setting instead of silently ignoring it.

## Performance

We are actively improving performance with the goal of catching up to and then
outperforming established inference engines, including
[vLLM](https://github.com/vllm-project/vllm) and
[MLX-LM](https://github.com/ml-explore/mlx-lm). See the
[libmir benchmark index](https://github.com/mirmir-tech/libmir/blob/main/benchmarks/index.md)
for current, reproducible comparisons on CUDA and Apple Silicon.

## Links

- [Documentation](https://docs.mirmir.tech/)
- [crates.io](https://crates.io/crates/mirmir)
- [Rust API documentation](https://docs.rs/mirmir)
- [Source and issues](https://github.com/mirmir-tech/mirmir)

Licensed under Apache-2.0.
