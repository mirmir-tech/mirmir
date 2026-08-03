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
