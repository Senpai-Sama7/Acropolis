//! # Dynamic Plugin Management
//!
//! This module provides the infrastructure for loading dynamic shared libraries
//! (`.so`, `.dll`, `.dylib`) as plugins at runtime. This allows extending the
//! system's capabilities without a recompile.

use crate::orchestrator::{AgentFactory, Orchestrator};
use anyhow::{anyhow, Result};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use tracing::{info, warn};

/// A function signature that plugins must export to register themselves.
/// The function takes a mutable reference to the registrar to add new capabilities.
type RegisterPluginFn = unsafe extern "C" fn(&mut PluginRegistrar);

/// # Plugin Registrar
///
/// An instance of this struct is passed to the plugin's registration function.
/// The plugin can then call methods on it to register new agent factories.
pub struct PluginRegistrar<'a> {
    orchestrator: &'a mut Orchestrator,
}

impl<'a> PluginRegistrar<'a> {
    /// Registers a new agent factory provided by the plugin.
    ///
    /// This method is called by the plugin to add its agents to the orchestrator.
    pub fn register_agent(&mut self, name: &str, factory: AgentFactory) {
        // We block on the async call here because this is part of the synchronous
        // FFI registration process. This is an acceptable use of `blocking_on`.
        tokio::runtime::Handle::current().block_on(async {
            if let Err(e) = self.orchestrator.register_agent(name, factory).await {
                warn!("Plugin failed to register agent '{}': {}", name, e);
            }
        });
    }
}

/// # Plugin Manager
///
/// Manages the loading and lifecycle of all loaded plugins.
pub struct PluginManager {
    /// A map of loaded libraries, keyed by their file path.
    /// This prevents loading the same library multiple times and keeps the
    /// library in memory. When the `Library` is dropped, the OS unloads it.
    loaded_plugins: HashMap<String, Library>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            loaded_plugins: HashMap::new(),
        }
    }

    /// Loads a plugin from a shared library file and registers its agents.
    ///
    /// # Safety
    /// This function is `unsafe` because it interfaces with arbitrary code
    /// from an external library via FFI (Foreign Function Interface).
    ///
    /// # Arguments
    /// * `path`: The file path to the shared library (`.so`, `.dll`, etc.).
    /// * `orchestrator`: A mutable reference to the orchestrator to register agents with.
    pub unsafe fn load(
        &mut self,
        path: &str,
        orchestrator: &mut Orchestrator,
    ) -> Result<()> {
        if self.loaded_plugins.contains_key(path) {
            warn!("Plugin at '{}' is already loaded. Skipping.", path);
            return Ok(());
        }

        info!("Loading plugin from: {}", path);

        // 1. Load the library into memory.
        let lib = Library::new(path)
            .map_err(|e| anyhow!("Failed to load library '{}': {}", path, e))?;

        // 2. Get a symbol for the registration function.
        let register_func: Symbol<RegisterPluginFn> = lib
            .get(b"register_plugin")
            .map_err(|e| {
                anyhow!(
                    "Could not find exported function 'register_plugin' in '{}': {}",
                    path,
                    e
                )
            })?;

        // 3. Create a registrar and call the plugin's function.
        let mut registrar = PluginRegistrar { orchestrator };
        register_func(&mut registrar);

        info!("✅ Successfully loaded and processed plugin '{}'.", path);

        // 4. Store the library to keep it loaded.
        self.loaded_plugins.insert(path.to_string(), lib);

        Ok(())
    }
}
