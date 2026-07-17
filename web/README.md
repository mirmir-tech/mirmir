# Mirmir Web

The browser UI is a Leptos CSR application built with Trunk. In addition to the
native Mirmir requirements, install the pinned toolchain's WebAssembly target
and the Trunk bundler:

```sh
rustup target add wasm32-unknown-unknown --toolchain nightly-2026-07-13
cargo install trunk --locked
```

Build the browser bundle from this directory:

```sh
trunk build
```

Use `trunk serve` for local frontend development. The native CLI and server do
not require Trunk or the WebAssembly target.
