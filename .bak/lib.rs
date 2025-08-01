//! # Adaptive Expert Platform - Core Library
//!
//! This crate provides the core functionalities for the agent orchestration platform,
//! including the `Orchestrator`, the `Agent` trait, the `PluginManager`, and the
//! shared `Memory` store. It is designed to be used as a library by other applications,
//! such as the `adaptive_expert_cli` or a future GUI.

// Declare all modules as public so they can be used by other crates.
pub mod agent;
pub mod memory;
pub mod orchestrator;
pub mod plugin;

// Re-export key types for easier access by library users.
pub use agent::Agent;
pub use memory::Memory;
pub use orchestrator::{Orchestrator, OrchestratorError};
pub use plugin::{PluginManager, PluginRegistrar};
