//! # In-Memory Storage
//!
//! This module provides a simple, thread-safe, in-memory key-value store.
//! It's designed to be a foundational component for agent memory, easily
//! replaceable by a more robust database solution in the future.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// # Memory Store
///
/// A thread-safe key-value store where keys are strings and values are
/// arbitrary JSON objects. The `Arc<Mutex<...>>` pattern is a standard
/// Rust idiom for safely sharing mutable state across async tasks.
#[derive(Debug, Clone)]
pub struct Memory {
    /// `Arc` allows multiple owners (e.g., multiple agents and the orchestrator).
    /// `Mutex` ensures that only one task can access the data at a time.
    store: Arc<Mutex<HashMap<String, Value>>>,
}

impl Memory {
    /// Creates a new, empty `Memory` instance.
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Sets a value for a given key.
    ///
    /// If the key already exists, its value will be overwritten.
    ///
    /// # Arguments
    /// * `key`: The key to associate with the value.
    /// * `value`: The `serde_json::Value` to store.
    pub async fn set(&self, key: &str, value: Value) -> Result<()> {
        // Lock the mutex to get exclusive access. The lock is released
        // automatically when `_guard` goes out of scope.
        let mut store_guard = self.store.lock().await;
        store_guard.insert(key.to_string(), value);
        debug!(key = key, "Set value in memory.");
        Ok(())
    }

    /// Retrieves a clone of the value for a given key.
    ///
    /// # Returns
    /// `Ok(Some(Value))` if the key exists, or `Ok(None)` if not.
    pub async fn get(&self, key: &str) -> Result<Option<Value>> {
        let store_guard = self.store.lock().await;
        // We clone the value so we can release the lock quickly.
        let value = store_guard.get(key).cloned();
        debug!(key = key, found = value.is_some(), "Get value from memory.");
        Ok(value)
    }

    /// Deletes a key-value pair from the store.
    ///
    /// # Returns
    /// `Ok(Some(Value))` containing the value that was removed,
    /// or `Ok(None)` if the key did not exist.
    pub async fn delete(&self, key: &str) -> Result<Option<Value>> {
        let mut store_guard = self.store.lock().await;
        let value = store_guard.remove(key);
        debug!(key = key, found = value.is_some(), "Deleted value from memory.");
        Ok(value)
    }
}
