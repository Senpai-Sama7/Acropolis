//! A simple thread‑safe key/value store for agents to persist JSON state.
//!
//! The initial implementation uses an in‑memory [`HashMap`] protected by a
//! [`tokio::sync::Mutex`].  Because the entire map is locked for every
//! operation, high concurrency workloads may experience contention.  In the
//! future this component could be swapped out for a more scalable store such
//! as a sharded map, or even an external vector or graph database.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// The memory store used by the orchestrator and agents.
///
/// Keys and values are arbitrary strings and JSON values respectively.  A
/// [`tokio::sync::Mutex`] is used here because the lock may be held across
/// `await` points when integrating with asynchronous code.  Clients should
/// clone the [`Memory`] handle as an [`Arc`] so that it can be shared across
/// tasks without additional synchronization.
pub struct Memory {
    store: Mutex<HashMap<String, Value>>,
}

impl Memory {
    /// Create a new, empty memory store.
    pub fn new() -> Self {
        Self { store: Mutex::new(HashMap::new()) }
    }

    /// Retrieve a value from the store by key.
    pub async fn get(&self, key: &str) -> Option<Value> {
        let guard = self.store.lock().await;
        guard.get(key).cloned()
    }

    /// Set a value in the store.  If a value previously existed for this key
    /// it will be overwritten.
    pub async fn set<V>(&self, key: &str, value: V)
    where
        V: Into<Value>,
    {
        let mut guard = self.store.lock().await;
        guard.insert(key.to_string(), value.into());
    }

    /// Delete a value from the store.  The previous value is returned if it
    /// existed.
    pub async fn delete(&self, key: &str) -> Option<Value> {
        let mut guard = self.store.lock().await;
        guard.remove(key)
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memory").finish_non_exhaustive()
    }
}