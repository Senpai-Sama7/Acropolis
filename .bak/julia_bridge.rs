//! Julia runtime wrapper for the Autonomous Adaptive Expert System (AAES).
//!
//! This module initializes a Julia runtime using the `jlrs` crate and
//! exposes methods to call specific functions defined in the Julia
//! scripts under `modules/julia`.  In particular it provides
//! functions to train and evaluate a CartPole policy (reinforcement
//! learning) and to call general scientific functions.  It also
//! demonstrates how to initialize the runtime with a specific
//! number of threads.

use anyhow::{anyhow, Result};
use jlrs::prelude::*;

/// A wrapper around the Julia runtime for calling Julia functions.
pub struct JuliaBridge {
    runtime: Julia,
}

impl JuliaBridge {
    /// Create a new Julia runtime with a default number of worker
    /// threads.  The runtime is initialized in multi-threaded mode
    /// (16 threads by default).  Initialization is unsafe and
    /// therefore wrapped in a safe function that returns a result.
    pub fn new() -> Result<Self> {
        // Initialize Julia with a custom thread count.  Safety: only
        // call init once per process; subsequent calls will fail.
        // Multi-threaded runtime requires Julia >= 1.9.
        let julia = unsafe { Julia::init(16) }.map_err(|e| anyhow!(e))?;
        Ok(Self { runtime: julia })
    }

    /// Train a CartPole policy for the given number of episodes and
    /// return the total reward.  This calls the Julia function
    /// `RLTraining.train_cartpole` defined in `rl_training.jl`.  The
    /// policy is saved to disk after training.
    pub fn train_cartpole(&self, episodes: i64) -> Result<f64> {
        let episodes = episodes as i64;
        let reward = self.runtime.scope(|mut frame| {
            let module = Module::main(&frame).submodule(&frame, "RLTraining")?;
            let func = module.function(&frame, "train_cartpole")?;
            let args = [Value::new(&mut frame, episodes)?];
            let ret = func.call(&mut frame, &args)?;
            let reward: f64 = ret.unwrap().unbox()?;
            Ok(reward)
        }).map_err(|e| anyhow!(e))?;
        Ok(reward)
    }

    /// Evaluate the current CartPole policy for the specified number
    /// of episodes and return the total reward.  This calls
    /// `RLTraining.evaluate_cartpole` in Julia.
    pub fn evaluate_cartpole(&self, episodes: i64) -> Result<f64> {
        let reward = self.runtime.scope(|mut frame| {
            let module = Module::main(&frame).submodule(&frame, "RLTraining")?;
            let func = module.function(&frame, "evaluate_cartpole")?;
            let args = [Value::new(&mut frame, episodes)?];
            let ret = func.call(&mut frame, &args)?;
            let reward: f64 = ret.unwrap().unbox()?;
            Ok(reward)
        }).map_err(|e| anyhow!(e))?;
        Ok(reward)
    }

    /// Optimize a simple quadratic function using Julia’s Optim.jl
    /// library.  Demonstrates calling a function that returns a
    /// tuple.  Returns `(x, f(x))`.
    pub fn optimize_quadratic(&self) -> Result<(f64, f64)> {
        let (x, fx) = self.runtime.scope(|mut frame| {
            let module = Module::main(&frame).submodule(&frame, "Optimization")?;
            let func = module.function(&frame, "optimize_quadratic")?;
            let ret = func.call0(&mut frame)?;
            // The returned value is a tuple of two f64 values.
            let tuple = ret.unwrap().cast::<Tuple2<f64, f64>>()?;
            Ok(tuple)
        }).map_err(|e| anyhow!(e))?;
        Ok((x, fx))
    }

    /// Simulate an exponential decay process using DifferentialEquations.jl.
    pub fn simulate_decay(&self, a: f64) -> Result<f64> {
        let final_val = self.runtime.scope(|mut frame| {
            let module = Module::main(&frame).submodule(&frame, "SimEngine")?;
            let func = module.function(&frame, "simulate_decay")?;
            let args = [Value::new(&mut frame, a)?];
            let ret = func.call(&mut frame, &args)?;
            let val: f64 = ret.unwrap().unbox()?;
            Ok(val)
        }).map_err(|e| anyhow!(e))?;
        Ok(final_val)
    }
}