use crate::memory::Memory;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tracing::{info, warn, error, instrument};

#[cfg(feature = "with-llama")]
use llama_cpp::{standard_sampler, LlamaModel, LlamaParams, SessionParams};

/// Enhanced Agent trait with better error handling and metadata
#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;
    fn agent_type(&self) -> &str;
    fn capabilities(&self) -> Vec<String>;
    async fn handle(&self, input: serde_json::Value, memory: Arc<Memory>) -> Result<String>;
    async fn health_check(&self) -> Result<AgentHealth>;
}

/// Agent health information
#[derive(Debug, Clone, Serialize)]
pub struct AgentHealth {
    pub status: String,
    pub uptime_seconds: u64,
    pub total_requests: u64,
    pub error_count: u64,
    pub average_response_time_ms: f64,
}

impl Default for AgentHealth {
    fn default() -> Self {
        Self {
            status: "healthy".to_string(),
            uptime_seconds: 0,
            total_requests: 0,
            error_count: 0,
            average_response_time_ms: 0.0,
        }
    }
}

// --- Built-in Agents ---

/// Simple echo agent for testing
pub struct EchoAgent {
    request_count: std::sync::atomic::AtomicU64,
    error_count: std::sync::atomic::AtomicU64,
    start_time: std::time::Instant,
}

impl EchoAgent {
    pub fn new() -> Self {
        Self {
            request_count: std::sync::atomic::AtomicU64::new(0),
            error_count: std::sync::atomic::AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        }
    }
}

#[async_trait]
impl Agent for EchoAgent {
    fn name(&self) -> &str { "echo" }

    fn agent_type(&self) -> &str { "utility" }

    fn capabilities(&self) -> Vec<String> {
        vec!["text_echo".to_string(), "testing".to_string()]
    }

    async fn handle(&self, input: serde_json::Value, _memory: Arc<Memory>) -> Result<String> {
        self.request_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let result = format!("Echo: {}", input.to_string());
        info!("Echo agent processed request");
        Ok(result)
    }

    async fn health_check(&self) -> Result<AgentHealth> {
        let uptime = self.start_time.elapsed().as_secs();
        let requests = self.request_count.load(std::sync::atomic::Ordering::Relaxed);
        let errors = self.error_count.load(std::sync::atomic::Ordering::Relaxed);

        Ok(AgentHealth {
            status: "healthy".to_string(),
            uptime_seconds: uptime,
            total_requests: requests,
            error_count: errors,
            average_response_time_ms: 1.0, // Echo is very fast
        })
    }
}

/// Enhanced Python tool agent with better security
pub struct PythonToolAgent {
    request_count: std::sync::atomic::AtomicU64,
    error_count: std::sync::atomic::AtomicU64,
    start_time: std::time::Instant,
    allowed_directories: Vec<String>,
    max_execution_time: std::time::Duration,
}

#[derive(Deserialize)]
struct PythonToolInput {
    script_path: String,
    args: Vec<String>,
    timeout_seconds: Option<u64>,
    working_directory: Option<String>,
}

impl PythonToolAgent {
    pub fn new() -> Self {
        Self {
            request_count: std::sync::atomic::AtomicU64::new(0),
            error_count: std::sync::atomic::AtomicU64::new(0),
            start_time: std::time::Instant::now(),
            allowed_directories: vec![
                "/tmp".to_string(),
                "/var/tmp".to_string(),
                std::env::current_dir().unwrap_or_default().to_string_lossy().to_string(),
            ],
            max_execution_time: std::time::Duration::from_secs(300), // 5 minutes
        }
    }

    fn validate_script_path(&self, path: &str) -> Result<()> {
        let path = std::path::Path::new(path);

        // Check if path is within allowed directories
        let is_allowed = self.allowed_directories.iter().any(|allowed| {
            path.starts_with(allowed)
        });

        if !is_allowed {
            return Err(anyhow!("Script path '{}' is not in allowed directories", path.display()));
        }

        // Check if file exists and is readable
        if !path.exists() {
            return Err(anyhow!("Script file '{}' does not exist", path.display()));
        }

        if !path.is_file() {
            return Err(anyhow!("Path '{}' is not a file", path.display()));
        }

        Ok(())
    }
}

#[async_trait]
impl Agent for PythonToolAgent {
    fn name(&self) -> &str { "python_tool" }

    fn agent_type(&self) -> &str { "execution" }

    fn capabilities(&self) -> Vec<String> {
        vec!["python_execution".to_string(), "script_runner".to_string()]
    }

    #[instrument(skip(self, _memory))]
    async fn handle(&self, input: serde_json::Value, _memory: Arc<Memory>) -> Result<String> {
        self.request_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let parsed_input: PythonToolInput = serde_json::from_value(input)
            .map_err(|e| {
                self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                anyhow!("Invalid Python tool input: {}", e)
            })?;

        // Validate script path
        self.validate_script_path(&parsed_input.script_path)?;

        info!(
            "Executing Python script: {} with args: {:?}",
            parsed_input.script_path,
            parsed_input.args
        );

        // Build command with security constraints
        let mut cmd = Command::new("python3");
        cmd.arg(&parsed_input.script_path);
        cmd.args(&parsed_input.args);

        // Set working directory if specified
        if let Some(work_dir) = parsed_input.working_directory {
            cmd.current_dir(work_dir);
        }

        // Set up I/O
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Execute with timeout
        let timeout = parsed_input.timeout_seconds
            .map(std::time::Duration::from_secs)
            .unwrap_or(self.max_execution_time);

        let output = tokio::time::timeout(timeout, cmd.output()).await
            .map_err(|_| {
                self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                anyhow!("Python script execution timed out after {:?}", timeout)
            })?
            .map_err(|e| {
                self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                anyhow!("Failed to execute Python script: {}", e)
            })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            info!("Python script executed successfully");
            Ok(stdout.to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Err(anyhow!("Python script failed: {}", stderr))
        }
    }

    async fn health_check(&self) -> Result<AgentHealth> {
        let uptime = self.start_time.elapsed().as_secs();
        let requests = self.request_count.load(std::sync::atomic::Ordering::Relaxed);
        let errors = self.error_count.load(std::sync::atomic::Ordering::Relaxed);

        Ok(AgentHealth {
            status: "healthy".to_string(),
            uptime_seconds: uptime,
            total_requests: requests,
            error_count: errors,
            average_response_time_ms: 100.0, // Python execution takes time
        })
    }
}

/// Enhanced LLM agent with better model management
#[cfg(feature = "with-llama")]
pub struct LlmAgent {
    name: String,
    model: LlamaModel,
    session_params: SessionParams,
    request_count: std::sync::atomic::AtomicU64,
    error_count: std::sync::atomic::AtomicU64,
    start_time: std::time::Instant,
    max_tokens: usize,
    temperature: f32,
}

#[cfg(feature = "with-llama")]
impl LlmAgent {
    pub fn new(name: &str, model_path: &str) -> Result<Self> {
        let params = LlamaParams::default()
            .with_model_path(model_path)
            .with_n_ctx(2048)
            .with_n_batch(512);

        let model = LlamaModel::load(params)?;
        let session_params = SessionParams::default()
            .with_seed(42);

        Ok(Self {
            name: name.to_string(),
            model,
            session_params,
            request_count: std::sync::atomic::AtomicU64::new(0),
            error_count: std::sync::atomic::AtomicU64::new(0),
            start_time: std::time::Instant::now(),
            max_tokens: 512,
            temperature: 0.7,
        })
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }
}

#[cfg(feature = "with-llama")]
#[async_trait]
impl Agent for LlmAgent {
    fn name(&self) -> &str { &self.name }

    fn agent_type(&self) -> &str { "llm" }

    fn capabilities(&self) -> Vec<String> {
        vec!["text_generation".to_string(), "completion".to_string(), "reasoning".to_string()]
    }

    #[instrument(skip(self, memory))]
    async fn handle(&self, input: serde_json::Value, memory: Arc<Memory>) -> Result<String> {
        self.request_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let prompt = input.get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                anyhow!("Missing 'prompt' field in LLM input")
            })?;

        // Get relevant context from memory
        let context = memory.search_memory(prompt, 3).await
            .unwrap_or_else(|_| vec![]);

        let enhanced_prompt = if context.is_empty() {
            prompt.to_string()
        } else {
            format!("Context:\n{}\n\nQuestion: {}",
                context.join("\n"), prompt)
        };

        info!("Generating LLM response for prompt: {}", &enhanced_prompt[..enhanced_prompt.len().min(100)]);

        // Generate response using llama.cpp
        let mut session = self.model.create_session(self.session_params.clone())?;

        let sampler = standard_sampler()
            .with_temperature(self.temperature)
            .with_top_p(0.9)
            .with_top_k(40);

        let response = session
            .infer(enhanced_prompt, sampler, |_| {})
            .map_err(|e| {
                self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                anyhow!("LLM inference failed: {}", e)
            })?;

        info!("LLM response generated successfully");
        Ok(response)
    }

    async fn health_check(&self) -> Result<AgentHealth> {
        let uptime = self.start_time.elapsed().as_secs();
        let requests = self.request_count.load(std::sync::atomic::Ordering::Relaxed);
        let errors = self.error_count.load(std::sync::atomic::Ordering::Relaxed);

        Ok(AgentHealth {
            status: "healthy".to_string(),
            uptime_seconds: uptime,
            total_requests: requests,
            error_count: errors,
            average_response_time_ms: 2000.0, // LLM inference takes time
        })
    }
}

/// Agent factory for creating agents by type
pub struct AgentFactory;

impl AgentFactory {
    pub fn create_agent(agent_type: &str, config: serde_json::Value) -> Result<Box<dyn Agent>> {
        match agent_type {
            "echo" => Ok(Box::new(EchoAgent::new())),
            "python" => Ok(Box::new(PythonToolAgent::new())),
            #[cfg(feature = "with-llama")]
            "llm" => {
                let name = config.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("llm_agent");
                let model_path = config.get("model_path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing 'model_path' for LLM agent"))?;

                let agent = LlmAgent::new(name, model_path)?;
                Ok(Box::new(agent))
            }
            _ => Err(anyhow!("Unknown agent type: {}", agent_type)),
        }
    }
}
