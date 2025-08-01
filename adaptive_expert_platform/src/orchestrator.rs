//! Core coordinator that routes tasks to agents (built-in or from plugins).

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn, error, instrument};

use crate::{
    agent::Agent,
    plugin::{self, PluginEvent, PluginSecurityConfig},
    settings::Settings,
    memory::Memory,
};

type Task = (String, Value, mpsc::Sender<Result<Value>>);

pub struct Orchestrator {
    agents: Arc<Mutex<HashMap<String, Arc<dyn Agent>>>>,
    memory: Arc<Memory>,
    plugin_security_config: PluginSecurityConfig,
    _bus: mpsc::Sender<PluginEvent>,
}

impl Orchestrator {
    #[instrument(skip(settings))]
    pub async fn new(settings: &Settings, memory: Arc<Memory>) -> Result<Self> {
        let (bus_tx, mut bus_rx) = mpsc::channel(16);
        let agents = Arc::new(Mutex::new(HashMap::new()));

        // Initialize plugin security configuration from settings
        let plugin_security_config = PluginSecurityConfig::from_security_config(&settings.security);

        // ---------- secure hot-reload loop ----------
        let agents_reload = agents.clone();
        let security_config_clone = plugin_security_config.clone();

        tokio::spawn(async move {
            while let Some(evt) = bus_rx.recv().await {
                match evt {
                    PluginEvent::Reload(path) => {
                        info!("Processing plugin reload: {:?}", path);

                        match unsafe { plugin::Plugin::load(&path, &security_config_clone) } {
                            Ok(lib) => {
                                match unsafe { lib.instantiate() } {
                                    Ok(agent) => {
                                        let name = agent.name().to_string();
                                        let metadata = lib.metadata();

                                        agents_reload.lock().await.insert(name.clone(), Arc::from(agent));
                                        info!(
                                            "Successfully reloaded plugin '{}' from {:?} (hash: {})",
                                            name, path, &metadata.hash[..16]
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to instantiate plugin agent from {:?}: {}", path, e);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to load plugin from {:?}: {}", path, e);
                            }
                        }
                    }
                    PluginEvent::SecurityViolation(msg) => {
                        warn!("Plugin security violation: {}", msg);
                        // TODO: Implement security incident logging/alerting
                    }
                }
            }
        });

                // start watcher task with security configuration
        let security_config_for_watcher = plugin_security_config.clone();
        let plugin_dir = settings.plugin_dir.clone();
        let bus_tx_clone = bus_tx.clone();

        tokio::spawn(async move {
            if let Err(e) = plugin::hot_reload::watch(
                plugin_dir,
                bus_tx_clone,
                security_config_for_watcher,
            ).await {
                error!("Plugin hot-reload watcher failed: {}", e);
            }
        });

        Ok(Self {
            agents,
            memory,
            plugin_security_config,
            _bus: bus_tx
        })
    }

    /// Dispatch a task `(agent_name, json_in)`; send result via `resp_tx`.
    #[instrument(skip(self, task), fields(agent_name))]
    pub async fn dispatch(&self, task: Task) -> Result<()> {
        let (name, input, resp_tx) = task;
        tracing::Span::current().record("agent_name", &name);

        let agent = {
            let map = self.agents.lock().await;
            match map.get(&name) {
                Some(agent) => agent.clone(),
                None => {
                    let error = anyhow::anyhow!("Unknown agent '{}'", name);
                    let _ = resp_tx.send(Err(error)).await;
                    return Ok(());
                }
            }
        }; // Release lock before awaiting

        // Execute agent with timeout and error handling
        let memory_clone = self.memory.clone();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30), // 30 second timeout
            agent.handle(input, memory_clone)
        ).await;

        let response = match result {
            Ok(Ok(output)) => Ok(Value::String(output)),
            Ok(Err(e)) => {
                error!("Agent '{}' execution failed: {}", name, e);
                Err(e)
            }
            Err(_) => {
                error!("Agent '{}' execution timed out", name);
                Err(anyhow::anyhow!("Agent execution timed out"))
            }
        };

        let _ = resp_tx.send(response).await;
        Ok(())
    }

    /// Register a built-in agent
    #[instrument(skip(self, agent))]
    pub async fn register_agent(&self, name: String, agent: Arc<dyn Agent>) -> Result<()> {
        info!("Registering built-in agent: {}", name);
        self.agents.lock().await.insert(name, agent);
        Ok(())
    }

    /// Get list of registered agent names
    pub async fn list_agents(&self) -> Vec<String> {
        self.agents.lock().await.keys().cloned().collect()
    }

    /// Get plugin security configuration
    pub fn plugin_security_config(&self) -> &PluginSecurityConfig {
        &self.plugin_security_config
    }

    /// Update plugin security configuration
    pub fn update_plugin_security_config(&mut self, config: PluginSecurityConfig) {
        self.plugin_security_config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::EchoAgent;
    use crate::memory::Memory;
    use crate::memory::redis_store::InMemoryEmbeddingCache;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_orchestrator_agent_registration() {
        let cache = Arc::new(InMemoryEmbeddingCache::new());
        let echo_agent = Arc::new(EchoAgent);
        let memory = Arc::new(Memory::new(
            echo_agent.clone(),
            echo_agent.clone(),
            cache,
        ));

        let settings = crate::settings::Settings::default();
        let orchestrator = Orchestrator::new(&settings, memory).await.unwrap();

        // Register an agent
        let agent = Arc::new(EchoAgent);
        orchestrator.register_agent("test_echo".to_string(), agent).await.unwrap();

        // Verify agent is registered
        let agents = orchestrator.list_agents().await;
        assert!(agents.contains(&"test_echo".to_string()));
    }

    #[tokio::test]
    async fn test_orchestrator_dispatch_timeout() {
        let cache = Arc::new(InMemoryEmbeddingCache::new());
        let echo_agent = Arc::new(EchoAgent);
        let memory = Arc::new(Memory::new(
            echo_agent.clone(),
            echo_agent.clone(),
            cache,
        ));

        let settings = crate::settings::Settings::default();
        let orchestrator = Orchestrator::new(&settings, memory).await.unwrap();

        // Test dispatching to non-existent agent
        let (tx, rx) = mpsc::channel(1);
        let task = ("nonexistent".to_string(), Value::String("test".to_string()), tx);

        orchestrator.dispatch(task).await.unwrap();
        let result = rx.recv().await.unwrap();
        assert!(result.is_err());
    }
}
