//! # Agent Definition and Implementations
//!
//! This module provides the `Agent` trait, the contract for all autonomous agents,
//! and built-in agent implementations like `EchoAgent` and the optional `LlmAgent`.

use crate::memory::Memory;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::info;

#[cfg(feature = "llama")]
use {
    anyhow::anyhow,
    llama_cpp::{LlamaModel, LlamaParams},
    tracing::warn,
};

/// # The Agent Trait
///
/// This defines the universal interface for any agent in the system.
/// It provides a name for identification and a handler for processing tasks.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Returns the unique name of the agent.
    fn name(&self) -> &str;

    /// The main entry point for the agent to handle a task.
    ///
    /// # Arguments
    /// * `input`: The string input or prompt for the agent.
    /// * `memory`: A shared handle to the orchestrator's memory store.
    ///
    /// # Returns
    /// A `Result<String>` containing the agent's output or an error.
    async fn handle(&self, input: &str, memory: Arc<Memory>) -> Result<String>;
}

// --- Built-in Agents ---

/// # Echo Agent
/// A simple agent that echoes back its input. Useful for testing.
pub struct EchoAgent;

#[async_trait]
impl Agent for EchoAgent {
    fn name(&self) -> &str {
        "echo"
    }

    async fn handle(&self, input: &str, _memory: Arc<Memory>) -> Result<String> {
        info!(agent = self.name(), "Echoing input.");
        Ok(input.to_string())
    }
}

/// # LLM-Powered Agent (Optional)
///
/// A concrete implementation of the `Agent` trait that uses a large language
/// model for its decision-making process. This is only available when the
/// `llama` feature is enabled.
#[cfg(feature = "llama")]
pub struct LlmAgent {
    model: LlamaModel,
}

#[cfg(feature = "llama")]
impl LlmAgent {
    /// Creates a new `LlmAgent` by loading a model from the given file path.
    ///
    /// # Returns
    /// A `Result` containing the new agent or an error if the model fails to load.
    pub fn new(model_path: &str) -> Result<Self> {
        info!(path = model_path, "Attempting to load LLM model.");
        let params = LlamaParams::default();
        let model = LlamaModel::load_from_file(model_path, params)
            .map_err(|e| anyhow!("Failed to load LLM model from '{}': {}", model_path, e))?;
        info!("✅ LLM model loaded successfully.");
        Ok(Self { model })
    }
}

#[cfg(feature = "llama")]
#[async_trait]
impl Agent for LlmAgent {
    fn name(&self) -> &str {
        "llm"
    }

    /// Implements the ReAct (Reason, Act) pattern by sending the input to the LLM.
    async fn handle(&self, input: &str, _memory: Arc<Memory>) -> Result<String> {
        info!(agent = self.name(), "Sending prompt to LLM.");

        // In a real scenario, this prompt would be more complex, incorporating
        // memory, goals, and observations.
        let prompt = format!("You are a helpful AI assistant. Respond to the following prompt: {}", input);

        let mut session = self.model.create_session(None);
        let response = session
            .prompt(&prompt, Default::default())
            .map_err(|e| anyhow!("LLM inference failed: {}", e))?;

        info!("Received response from LLM.");
        Ok(response.trim().to_string())
    }
}
