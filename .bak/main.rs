//! # Adaptive Expert Platform - Command-Line Interface
//!
//! This is the main entry point for running the agent orchestration service.
//! It is responsible for setting up the environment, loading plugins,
//! registering built-in agents, and running a demonstration of the orchestrator.

use adaptive_expert_core::agent::{EchoAgent, LlmAgent};
use adaptive_expert_core::orchestrator::Orchestrator;
use adaptive_expert_core::plugin::PluginManager;
use anyhow::{Context, Result};
use std::env;
use std::fs;
use tracing::{error, info, warn};

/// The directory where the CLI will look for plugins to load.
const PLUGIN_DIR: &str = "plugins";

#[tokio::main]
async fn main() -> Result<()> {
    // --- 1. Initialize Tracing ---
    // Sets up a simple subscriber that logs to the console.
    // The `RUST_LOG` environment variable can be used to set the log level.
    // (e.g., `RUST_LOG=info,adaptive_expert_core=debug`).
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("🔥 Initializing Adaptive Expert Platform CLI...");

    // --- 2. Create Core Components ---
    let mut orchestrator = Orchestrator::new();
    let mut plugin_manager = PluginManager::new();

    // --- 3. Register Built-in Agents ---
    info!("Registering built-in agents...");
    orchestrator
        .register_agent("echo", || Box::new(EchoAgent))
        .await?;

    // --- 4. Register LLM Agent (if enabled and configured) ---
    #[cfg(feature = "llama")]
    {
        if let Ok(model_path) = env::var("LLM_MODEL_PATH") {
            match LlmAgent::new(&model_path) {
                Ok(llm_agent) => {
                    orchestrator
                        .register_agent("llm", move || Box::new(LlmAgent::new(&model_path).unwrap()))
                        .await?;
                }
                Err(e) => error!("Failed to initialize LLM agent: {}", e),
            }
        } else {
            warn!("'llama' feature is enabled, but LLM_MODEL_PATH environment variable is not set.");
        }
    }

    // --- 5. Load Plugins from Filesystem ---
    info!("Loading plugins from './{}' directory...", PLUGIN_DIR);
    if let Ok(entries) = fs::read_dir(PLUGIN_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if matches!(ext, "so" | "dll" | "dylib") {
                    unsafe {
                        if let Err(e) = plugin_manager.load(path.to_str().unwrap(), &mut orchestrator) {
                            error!("Failed to load plugin at {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    } else {
        warn!("'{}' directory not found. No plugins will be loaded.", PLUGIN_DIR);
    }

    // --- 6. Run a Demonstration ---
    info!("\n--- Running Orchestrator Demonstration ---");

    // Example 1: Call the built-in echo agent
    let echo_input = "Hello from the CLI!";
    info!("Calling 'echo' agent with input: '{}'", echo_input);
    match orchestrator.call_agent("echo", echo_input).await {
        Ok(output) => info!("✅ 'echo' agent responded: '{}'", output),
        Err(e) => error!("❌ 'echo' agent failed: {}", e),
    }

    // Example 2: Call the plugin-provided math agent
    let add_input = "10,32";
    info!("\nCalling 'add' agent (from plugin) with input: '{}'", add_input);
    match orchestrator.call_agent("add", add_input).await {
        Ok(output) => info!("✅ 'add' agent responded: '{}'", output),
        Err(e) => error!("❌ 'add' agent failed: {}", e),
    }

    // Example 3: Call the LLM agent if it was registered
    if env::var("LLM_MODEL_PATH").is_ok() {
        let llm_input = "What is the capital of France?";
        info!("\nCalling 'llm' agent with input: '{}'", llm_input);
        match orchestrator.call_agent("llm", llm_input).await {
            Ok(output) => info!("✅ 'llm' agent responded: '{}'", output),
            Err(e) => error!("❌ 'llm' agent failed: {}", e),
        }
    }

    info!("\n--- Demonstration Complete ---");
    Ok(())
}
