//! HTTP server with REST API for agent management and task execution.

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::{StatusCode, HeaderMap},
    middleware,
    response::{Json, IntoResponse},
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, instrument};

use crate::{
    agent::Agent,
    auth::{AuthManager, Claims, LoginRequest, LoginResponse, auth_middleware},
    middleware::{
        create_cors_layer, create_rate_limiter, create_body_limit_layer,
        rate_limit_middleware, security_headers_middleware, security_logging_middleware
    },
    orchestrator::Orchestrator,
    settings::Settings,
    memory::Memory,
};

/// Application state shared across HTTP handlers
#[derive(Clone)]
pub struct AppState {
    pub orchestrator: Arc<RwLock<Orchestrator>>,
    pub auth_manager: Arc<AuthManager>,
    pub rate_limiter: Arc<crate::middleware::AppRateLimiter>,
    pub settings: Settings,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_seconds: u64,
    agent_count: usize,
    memory_fragments: usize,
}

/// Agent registration request
#[derive(Deserialize)]
struct RegisterAgentRequest {
    name: String,
    agent_type: String,
    config: serde_json::Value,
}

/// Task execution request
#[derive(Deserialize)]
struct ExecuteTaskRequest {
    agent_name: String,
    input: serde_json::Value,
    timeout_seconds: Option<u64>,
}

/// Task execution response
#[derive(Serialize)]
struct ExecuteTaskResponse {
    success: bool,
    result: Option<String>,
    error: Option<String>,
    execution_time_ms: u64,
}

/// Agent information
#[derive(Serialize)]
struct AgentInfo {
    name: String,
    agent_type: String,
    status: String,
}

/// Memory statistics
#[derive(Serialize)]
struct MemoryStats {
    total_fragments: usize,
    cache_hit_rate: f64,
    memory_usage_mb: f64,
}

/// Create the HTTP router with all endpoints and security middleware
pub fn create_router(state: AppState) -> Router {
    // Create CORS layer based on security configuration
    let cors_layer = create_cors_layer(&state.settings.security);

    // Create body size limit layer
    let body_limit_layer = create_body_limit_layer(state.settings.security.max_request_size_mb);

    // Public routes (no authentication required)
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/auth/login", post(login));

    // Protected routes (authentication required)
    let protected_routes = Router::new()
        .route("/agents", get(list_agents))
        .route("/agents", post(register_agent))
        .route("/agents/:name", delete(remove_agent))
        .route("/execute", post(execute_task))
        .route("/memory/stats", get(memory_stats))
        .route("/memory/search", post(search_memory))
        .route("/memory/add", post(add_memory))
        .route("/metrics", get(get_metrics))
        .route("/auth/users", post(create_user)) // Admin only
        .route("/auth/password", post(change_password))
        .layer(middleware::from_fn_with_state(
            state.auth_manager.clone(),
            auth_middleware
        ));

    // Combine routes and apply middleware layers
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(
            state.rate_limiter.clone(),
            rate_limit_middleware
        ))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(middleware::from_fn(security_logging_middleware))
        .layer(cors_layer)
        .layer(body_limit_layer);

    app
}

/// Health check endpoint
#[instrument(skip(state))]
async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let orchestrator = state.orchestrator.read().await;
    let agent_count = orchestrator.list_agents().len();

    // TODO: Get actual uptime and memory stats
    let response = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: 0, // TODO: Track actual uptime
        agent_count,
        memory_fragments: 0, // TODO: Get from memory
    };

    Ok(Json(response))
}

/// List all registered agents
#[instrument(skip(state))]
async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentInfo>>, StatusCode> {
    let orchestrator = state.orchestrator.read().await;
    let agents = orchestrator.list_agents();

    let agent_infos: Vec<AgentInfo> = agents
        .into_iter()
        .map(|name| AgentInfo {
            name,
            agent_type: "unknown".to_string(), // TODO: Track agent types
            status: "active".to_string(),
        })
        .collect();

    Ok(Json(agent_infos))
}

/// Register a new agent
#[instrument(skip(state))]
async fn register_agent(
    State(state): State<AppState>,
    Json(request): Json<RegisterAgentRequest>,
) -> Result<StatusCode, StatusCode> {
    let mut orchestrator = state.orchestrator.write().await;

    // TODO: Implement agent registration based on type
    match request.agent_type.as_str() {
        "echo" => {
            use crate::agent::EchoAgent;
            orchestrator.register_agent(Arc::new(EchoAgent));
        }
        "python" => {
            use crate::agent::PythonToolAgent;
            orchestrator.register_agent(Arc::new(PythonToolAgent::new()));
        }
        _ => {
            warn!("Unknown agent type: {}", request.agent_type);
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    info!("Registered agent: {}", request.name);
    Ok(StatusCode::CREATED)
}

/// Remove an agent
#[instrument(skip(state))]
async fn remove_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mut orchestrator = state.orchestrator.write().await;

    // TODO: Implement agent removal
    warn!("Agent removal not yet implemented: {}", name);
    Ok(StatusCode::OK)
}

/// Execute a task with an agent
#[instrument(skip(state))]
async fn execute_task(
    State(state): State<AppState>,
    Json(request): Json<ExecuteTaskRequest>,
) -> Result<Json<ExecuteTaskResponse>, StatusCode> {
    let start_time = std::time::Instant::now();
    let orchestrator = state.orchestrator.read().await;

    // Create a dummy memory for now
    let dummy_memory = Arc::new(create_dummy_memory());

    let result = orchestrator.dispatch((
        request.agent_name.clone(),
        request.input,
        tokio::sync::mpsc::channel(1).0,
    )).await;

    let execution_time = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(_) => {
            // TODO: Get actual result from channel
            Ok(Json(ExecuteTaskResponse {
                success: true,
                result: Some("Task executed successfully".to_string()),
                error: None,
                execution_time_ms: execution_time,
            }))
        }
        Err(e) => {
            error!("Task execution failed: {}", e);
            Ok(Json(ExecuteTaskResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                execution_time_ms: execution_time,
            }))
        }
    }
}

/// Get memory statistics
#[instrument(skip(state))]
async fn memory_stats(
    State(state): State<AppState>,
) -> Result<Json<MemoryStats>, StatusCode> {
    // TODO: Get actual memory stats
    let stats = MemoryStats {
        total_fragments: 0,
        cache_hit_rate: 0.0,
        memory_usage_mb: 0.0,
    };

    Ok(Json(stats))
}

/// Search memory
#[instrument(skip(state))]
async fn search_memory(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let query = request.get("query")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // TODO: Use actual memory instance
    let dummy_memory = create_dummy_memory();
    let results = dummy_memory.search_memory(query, 10).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(results))
}

/// Add content to memory
#[instrument(skip(state))]
async fn add_memory(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    let content = request.get("content")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // TODO: Use actual memory instance
    let dummy_memory = create_dummy_memory();
    dummy_memory.add_memory(content).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::CREATED)
}

/// Get system metrics
#[instrument(skip(state))]
async fn get_metrics(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // TODO: Implement actual metrics collection
    let metrics = serde_json::json!({
        "requests_per_second": 0,
        "average_response_time_ms": 0,
        "error_rate": 0.0,
        "memory_usage_mb": 0.0,
        "cpu_usage_percent": 0.0,
    });

    Ok(Json(metrics))
}

/// Login endpoint
#[instrument(skip(state, request))]
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    match state.auth_manager.authenticate(&request.username, &request.password).await {
        Ok(token) => {
            let claims = state.auth_manager.validate_token(&token)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let response = LoginResponse {
                token,
                expires_in: state.settings.security.jwt_expiry_hours * 3600, // Convert to seconds
                user_id: claims.sub,
                roles: claims.roles,
            };

            info!("User {} logged in successfully", request.username);
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Login failed for user {}: {}", request.username, e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Create new user endpoint (admin only)
#[instrument(skip(state, request))]
async fn create_user(
    State(state): State<AppState>,
    Json(request): Json<CreateUserRequest>,
) -> Result<StatusCode, StatusCode> {
    // This endpoint should only be accessible by admins
    // Role-based authorization would be handled by middleware

    match state.auth_manager.add_user(
        request.username,
        &request.password,
        request.roles
    ).await {
        Ok(_) => {
            info!("User {} created successfully", request.username);
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            error!("Failed to create user {}: {}", request.username, e);
            Err(StatusCode::CONFLICT)
        }
    }
}

/// Change password endpoint
#[instrument(skip(state, request))]
async fn change_password(
    State(state): State<AppState>,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<StatusCode, StatusCode> {
    match state.auth_manager.update_password(&request.username, &request.new_password).await {
        Ok(_) => {
            info!("Password changed for user {}", request.username);
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("Failed to change password for user {}: {}", request.username, e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Create user request
#[derive(Deserialize)]
struct CreateUserRequest {
    username: String,
    password: String,
    roles: Vec<String>,
}

/// Change password request
#[derive(Deserialize)]
struct ChangePasswordRequest {
    username: String,
    new_password: String,
}

/// Create a dummy memory instance for testing
fn create_dummy_memory() -> Memory {
    use crate::memory::redis_store::InMemoryEmbeddingCache;
    use crate::agent::EchoAgent;

    let cache = Arc::new(InMemoryEmbeddingCache::new());
    let echo_agent = Arc::new(EchoAgent);
    Memory::new(echo_agent.clone(), echo_agent, cache)
}

/// Start the HTTP server and wait for shutdown signal
pub async fn serve(settings: &Settings) -> Result<()> {
    info!("Starting HTTP server on port {}", settings.server.port);

    // Validate security configuration
    if settings.security.enable_authentication && settings.security.jwt_secret.is_none() {
        return Err(anyhow::anyhow!("JWT secret must be provided when authentication is enabled"));
    }

    // Create application state
    let memory = Arc::new(create_dummy_memory());
    let orchestrator = Arc::new(RwLock::new(
        Orchestrator::new(&settings, memory).await
            .map_err(|e| {
                error!("Failed to initialize orchestrator: {}", e);
                anyhow::anyhow!("Orchestrator initialization failed")
            })?
    ));

    // Initialize authentication manager
    let jwt_secret = settings.security.jwt_secret.clone()
        .unwrap_or_else(|| {
            warn!("No JWT secret provided, using default (INSECURE for production)");
            "default_insecure_secret_change_in_production".to_string()
        });
    let auth_manager = Arc::new(AuthManager::new(jwt_secret));

    // Initialize rate limiter
    let rate_limiter = create_rate_limiter(&settings.security);

    let state = AppState {
        orchestrator,
        auth_manager,
        rate_limiter,
        settings: settings.clone(),
    };

    // Create router
    let app = create_router(state);

    // Bind to address
    let addr = format!("{}:{}", settings.server.host, settings.server.port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid server address: {}", e))?;

    info!("HTTP server listening on {}", addr);

    // Start server with graceful shutdown
    let server = axum::Server::bind(&addr)
        .serve(app.into_make_service());

    // Wait for shutdown signal
    let graceful = server.with_graceful_shutdown(wait_for_shutdown());

    if let Err(e) = graceful.await {
        error!("HTTP server error: {}", e);
    }

    info!("HTTP server shutdown complete");
    Ok(())
}

/// Wait for shutdown signal (Ctrl+C)
async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        let mut sigint = signal(SignalKind::interrupt()).unwrap();

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down gracefully");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C), shutting down gracefully");
            }
        }
    }

    #[cfg(not(unix))]
    {
        // For Windows, we can use a simple approach
        tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;
    }
}
