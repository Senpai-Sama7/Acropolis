//! Julia runtime integration with enhanced security and proper dependency handling.

#[cfg(feature = "with-julia")]
mod julia_impl {
    use crate::{agent::Agent, memory::Memory};
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use jlrs::prelude::*;
    use serde_json::Value;
    use std::sync::Arc;
    use tokio::sync::{mpsc::Sender, oneshot::channel};
    use tracing::{info, error};

    pub struct JuliaTask {
        pub function_name: String,
        pub json_config: Value,
        pub response: oneshot::Sender<Result<String>>,
    }

    /// Initialize Julia runtime in a dedicated thread with bounded queue for concurrency control
    fn init_julia() -> Result<Sender<JuliaTask>> {
        // Use a bounded channel with backpressure to limit concurrent Julia tasks
        let (tx, mut rx) = tokio::sync::mpsc::channel::<JuliaTask>(100);

        std::thread::spawn(move || {
            let mut julia = RuntimeBuilder::new().start().expect("Could not init Julia");
            let mut frame = StackFrame::new();

            info!("Julia runtime initialized in dedicated thread");

            // Load common Julia modules
            if let Err(e) = julia.scope(|mut frame| {
                let include = Module::main(&mut frame).function(&mut frame, "include")?;
                // Load model files if they exist
                for model_path in &["models/julia/causal_model.jl", "models/julia/ltn_logic.jl"] {
                    if std::path::Path::new(model_path).exists() {
                        include.call1(&mut frame, Value::new(&mut frame, model_path))?;
                        info!("Loaded Julia model: {}", model_path);
                    }
                }
                Ok(())
            }) {
                error!("Failed to load Julia models: {}", e);
            }

            // Main loop for processing Julia tasks
            while let Some(task) = rx.blocking_recv() {
                let result = julia.scope(|mut frame| {
                    let func = Module::main(&mut frame).function(&mut frame, &task.function_name)?;
                    let config_val = Value::new(&mut frame, task.json_config);
                    let result = func.call1(&mut frame, config_val)?;
                    Ok(result.display_string(&mut frame)?)
                });

                let response = match result {
                    Ok(output) => Ok(output),
                    Err(e) => Err(anyhow!("Julia execution error: {}", e)),
                };

                if let Err(_) = task.response.send(response) {
                    error!("Failed to send Julia task response");
                }
            }
        });

        Ok(tx)
    }

    /// Allowed Julia function names for security
    const ALLOWED_JULIA_FUNCTIONS: &[&str] = &[
        "main",
        "run_model", 
        "predict",
        "train_model",
        "causal_analysis",
        "ltn_inference",
        "clip_encode",
        "process_data",
    ];

    /// Julia agent that processes tasks via the runtime
    pub struct JuliaAgent {
        sender: Sender<JuliaTask>,
    }

    impl JuliaAgent {
        pub fn new() -> Result<Self> {
            let sender = init_julia()?;
            Ok(Self { sender })
        }
    }

    #[async_trait]
    impl Agent for JuliaAgent {
        fn name(&self) -> &str {
            "julia"
        }

        async fn handle(&self, input: Value, _memory: Arc<Memory>) -> Result<String> {
            let function_name = input.get("function")
                .and_then(|v| v.as_str())
                .unwrap_or("main")
                .to_string();

            // Validate function name against allowlist
            if !ALLOWED_JULIA_FUNCTIONS.contains(&function_name.as_str()) {
                return Err(anyhow!(
                    "Julia function '{}' not allowed. Permitted functions: {:?}", 
                    function_name, 
                    ALLOWED_JULIA_FUNCTIONS
                ));
            }

            let config = input.get("config")
                .cloned()
                .unwrap_or(input);

            let (response_tx, response_rx) = oneshot::channel();
            let task = JuliaTask {
                function_name,
                json_config: config,
                response: response_tx,
            };

            // Send with timeout to handle backpressure gracefully
            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.sender.send(task)
            ).await {
                Ok(Ok(())) => {},
                Ok(Err(_)) => return Err(anyhow!("Julia runtime channel closed")),
                Err(_) => return Err(anyhow!("Julia task queue full - request timed out")),
            }

            response_rx.await
                .map_err(|_| anyhow!("Julia task response channel closed"))?
        }
    }
}

#[cfg(feature = "with-julia")]
pub use julia_impl::JuliaAgent;

#[cfg(not(feature = "with-julia"))]
pub struct JuliaAgent;

#[cfg(not(feature = "with-julia"))]
impl JuliaAgent {
    pub fn new() -> Result<Self, anyhow::Error> {
        Err(anyhow::anyhow!("Julia support not compiled in. Enable 'with-julia' feature."))
    }
}
