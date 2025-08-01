//! # The Central Orchestrator
//!
//! This module defines the `Orchestrator`, the core component responsible for
//! managing the agent registry and dispatching tasks to agents.

use crate::agent::Agent;
use crate::memory::Memory;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A factory function for creating new agent instances.
pub type AgentFactory = Box<dyn Fn() -> Box<dyn Agent> + Send + Sync>;

/// Errors that can occur within the Orchestrator.
#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("Agent '{0}' not found in registry.")]
    AgentNotFound(String),
    #[error("Agent '{0}' is already registered.")]
    AgentAlreadyRegistered(String),
}

/// # Orchestrator
///
/// The main struct that holds the state of the running system. It manages a
/// registry of available agents and provides a method to call them.
pub struct Orchestrator {
    /// The shared, thread-safe memory store for all agents.
    memory: Arc<Memory>,
    /// A registry of agent factories, keyed by agent name.
    /// `RwLock` allows multiple concurrent reads, which is common for `call_agent`.
    agents: Arc<RwLock<HashMap<String, AgentFactory>>>,
}

impl Orchestrator {
    /// Creates a new Orchestrator instance.
    ///
    /// Initializes the agent registry and the shared memory store.
    pub fn new() -> Self {
        info!("Initializing new orchestrator.");
        Self {
            memory: Arc::new(Memory::new()),
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a new agent factory with the orchestrator.
    ///
    /// This allows the orchestrator to create instances of the agent on demand.
    ///
    /// # Arguments
    /// * `name`: The unique name to identify the agent.
    /// * `factory`: A closure that creates a `Box<dyn Agent>`.
    pub async fn register_agent(
        &mut self,
        name: &str,
        factory: AgentFactory,
    ) -> Result<(), OrchestratorError> {
        let mut agents = self.agents.write().await;
        if agents.contains_key(name) {
            warn!(agent = name, "Attempted to register an agent that already exists.");
            return Err(OrchestratorError::AgentAlreadyRegistered(name.to_string()));
        }
        agents.insert(name.to_string(), factory);
        info!(agent = name, "Successfully registered new agent.");
        Ok(())
    }

    /// Calls an agent by name to handle a task.
    ///
    /// It looks up the agent in the registry, creates a new instance,
    /// and calls its `handle` method.
    ///
    /// # Arguments
    /// * `agent_name`: The name of the agent to call.
    /// * `input`: The input string to pass to the agent's handler.
    ///
    /// # Returns
    /// A `Result` containing the agent's string output or an error.
    pub async fn call_agent(&self, agent_name: &str, input: &str) -> Result<String> {
        let agents = self.agents.read().await;
        let factory = agents
            .get(agent_name)
            .ok_or_else(|| OrchestratorError::AgentNotFound(agent_name.to_string()))?;

        // Create a new instance of the agent for this call.
        let agent = factory();
        info!(agent = agent.name(), "Dispatching task.");

        // Call the agent's handler with the input and a clone of the memory handle.
        agent.handle(input, self.memory.clone()).await
    }
}
