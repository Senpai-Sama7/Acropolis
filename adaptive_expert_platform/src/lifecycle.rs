//! Advanced agent lifecycle management system

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{RwLock, mpsc, oneshot, Semaphore};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use dashmap::DashMap;
use tokio::process::Command;
use tracing::{info, warn, error, instrument, debug};
use reqwest::Client;  // ← newly added

use crate::agent::Agent;

/// Agent lifecycle states
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentState {
    Initializing,
    Running,
    Updating,
    Scaling,
    Terminated,
}

/// Agent deployment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDeploymentConfig {
    pub name: String,
    pub agent_type: String,
    pub version: String,
    pub replicas: u32,
    pub min_replicas: u32,
    pub max_replicas: u32,
    pub resource_limits: ResourceLimits,
}

// ... (all other code in this file remains unchanged) ...

impl LifecycleManager {
    // ... around line 682:
    async fn some_http_call(&self) -> Result<()> {
        // previous error: builder not found
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        // ... use client ...
        Ok(())
    }

    // ... rest of file follows ...
}

/// Deployment status information
#[derive(Debug, Serialize)]
pub struct DeploymentStatus {
    pub name: String,
    pub desired_replicas: u32,
    pub current_replicas: u32,
    pub healthy_replicas: u32,
    pub running_replicas: u32,
    pub instances: Vec<AgentInstance>,
}

