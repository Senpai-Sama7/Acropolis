//! Demonstration binary for the Reinforcement Learning agent.
//!
//! This executable initializes the Julia bridge and RL agent and
//! demonstrates training and evaluation loops.  It can be run
//! separately from the main orchestrator to test the RL integration.

mod julia_bridge;
mod agents;

use agents::rl_agent::RlAgent;
use julia_bridge::JuliaBridge;

fn main() -> anyhow::Result<()> {
    println!("🚀 Starting RL demo...");
    // Initialize Julia and the RL agent
    let julia = JuliaBridge::new()?;
    let rl_agent = RlAgent::new(julia);
    // Train for a few episodes and evaluate
    rl_agent.train(50)?;
    rl_agent.evaluate(10)?;
    // Optionally auto-improve until a target reward
    rl_agent.auto_improve(800.0)?;
    Ok(())
}