# Adaptive Expert Platform

The **Adaptive Expert Platform** is a modular, cross‑domain agent framework
written in Rust.  It orchestrates multiple agents—LLM‑backed or otherwise—via
an asynchronous message bus, supports runtime extension through native and
WebAssembly plugins, and provides a simple in‑memory JSON store for shared
state.  This repository contains the core orchestrator, a sample plugin and
tests demonstrating end‑to‑end operation.

## Features

* **Asynchronous orchestration** – built on [`tokio`](https://tokio.rs/) with
  an `mpsc` channel dispatch loop.  Agents run concurrently and non‑blocking.
* **Plugin system** – dynamically load `.so`, `.dylib` or `.dll` files at
  runtime using [`libloading`](https://docs.rs/libloading).  Plugins register
  new agents via a safe `PluginRegistrar` interface.
* **Optional LLM integration** – enable the `llama` feature to compile in
  support for [llama.cpp](https://github.com/ggerganov/llama.cpp) through the
  `llama_cpp` crate and build your own local LLM agents.
* **Optional WASM plugins** – enable the `wasm` feature to load WebAssembly
  modules via [`wasmtime`](https://github.com/bytecodealliance/wasmtime).
* **Optional Zig FFI** – enable the `zig` feature and link against a Zig‑compiled
  dynamic library to expose agents written in Zig.  The sample `ZigAgent`
  demonstrates how to call a Zig function exported to C【486881172987055†L52-L86】.
* **Optional Julia interop** – enable the `julia` feature to embed the Julia
  runtime via the [`jlrs`](https://docs.rs/jlrs) crate and call Julia
  functions from Rust.  The sample `JuliaAddAgent` uses Julia’s `+`
  function to add two numbers【521504590463878†L375-L446】.
* **Thread‑safe memory store** – a `tokio::Mutex`‑protected `HashMap` stores
  arbitrary `serde_json::Value`s.  Agents can persist and retrieve state.
* **Cross‑platform** – tested on Linux, macOS and Windows.  The build script
  handles plugin file extensions automatically.

## Directory Structure

```
adaptive_expert_platform/
├── Cargo.toml            # Package manifest and workspace configuration
├── build.sh              # One‑step build and test script
├── src/
│   ├── lib.rs            # Crate root re‑exporting modules
│   ├── main.rs           # Binary that sets up logging, loads plugins and runs the orchestrator
│   ├── agent.rs          # Agent trait and built‑in agents (Echo, optional LLM)
│   ├── ffi_zig.rs        # Zig FFI bindings and sample agent (feature‑gated)
│   ├── ffi_julia.rs      # Julia interop and sample agent (feature‑gated)
│   ├── orchestrator.rs   # Core orchestration logic (message bus, plugin loading)
│   ├── plugin.rs         # Plugin registration types
│   ├── wasm_plugin.rs    # Wasm component loader via wasmtime (feature‑gated)
│   └── memory.rs         # Shared JSON memory implementation
├── plugins/
│   └── uppercase_plugin/ # Example plugin crate producing a dynamic library
├── tests/
│   └── basic_agent_test.rs # End‑to‑end tests for echo and plugin loading
└── README.md             # This file
```

## Building and Running

Clone the repository and run the build script from the project root:

```bash
./build.sh
```

The script will:

1. Compile the Rust workspace in release mode, including the `uppercase_plugin`.
2. Detect your operating system and copy the compiled plugin into the top‑level
   `plugins/` directory.
3. Run the test suite with `cargo test --release`.

After the script completes you can run the orchestrator binary directly:

```bash
cargo run --release
```

By default the binary registers the built‑in echo agent and loads any plugins
found in the `plugins/` directory.  You can interact with the orchestrator
programmatically via the `call_agent` method on the `Orchestrator` type.

## Writing Your Own Agents

Agents implement the asynchronous [`Agent`](src/agent.rs) trait:

```rust
use adaptive_expert_platform::agent::Agent;
use adaptive_expert_platform::memory::Memory;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct MyAgent;

#[async_trait]
impl Agent for MyAgent {
    fn name(&self) -> &str { "my_agent" }
    async fn handle(&self, input: &str, memory: Arc<Memory>) -> Result<String> {
        // Business logic here
        Ok(format!("got: {}", input))
    }
}
```

Register your agent with the orchestrator at startup:

```rust
let mut orchestrator = Orchestrator::new();
orchestrator.register_agent("my_agent", || Box::new(MyAgent));
```

## Writing Plugins

Plugins are separate crates compiled as `cdylib`s.  They must define a
`register_plugin` function with the C ABI that accepts a mutable
[`PluginRegistrar`].  Within this function call `register_agent` on the
registrar to register new agents.  See the
[`uppercase_plugin`](plugins/uppercase_plugin/src/lib.rs) for a complete example.

```rust
use adaptive_expert_platform::plugin::PluginRegistrar;
use adaptive_expert_platform::agent::Agent;
use adaptive_expert_platform::memory::Memory;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct MyPluginAgent;

#[async_trait]
impl Agent for MyPluginAgent {
    fn name(&self) -> &str { "my_plugin_agent" }
    async fn handle(&self, input: &str, _memory: Arc<Memory>) -> Result<String> {
        Ok(input.chars().rev().collect())
    }
}

#[no_mangle]
pub extern "C" fn register_plugin(registrar: &mut PluginRegistrar) {
    registrar.register_agent("reverse", || Box::new(MyPluginAgent));
}
```

Compile your plugin with `cargo build --release --crate-type=cdylib` and
copy the resulting dynamic library into the `plugins/` directory.

## Prompt Template Summary

You are a Senior Systems Architect & Software Engineer AI.  Your task: generate
a fully working, production‑ready codebase for the “Adaptive Expert Platform”
LLM Agent Orchestrator Core in Rust, including async orchestration, plugin
loading (.so/.dll + WASM), LLM agents (llama_cpp), in‑memory JSON memory,
ReAct pattern, end‑to‑end tests, and one‑step build & test scripts.  Output
must include: full complete real and working codebase; tests; README.md with
usage and template summary.  Use Tokio, tracing_subscriber, libloading,
async_trait, anyhow, serde_json, llama_cpp.  No placeholders or mocks.  Ensure
code compiles and tests pass.  End with this prompt template summary for reuse.

Feel free to adapt this template for other modules (e.g. Zig FFI, Julia plugin,
WebGPU UI).