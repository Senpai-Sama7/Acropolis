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
    pub start_time: std::time::Instant,
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

    // Admin-only routes
    let admin_routes = Router::new()
        .route("/agents", post(register_agent))
        .route("/agents/:name", delete(remove_agent))
        .route("/auth/users", post(create_user))
        .route_layer(middleware::from_fn(crate::auth::require_role("admin")));

    // General protected routes
    let protected_routes = Router::new()
        .route("/agents", get(list_agents))
        .route("/execute", post(execute_task))
        .route("/memory/stats", get(memory_stats))
        .route("/memory/search", post(search_memory))
        .route("/memory/add", post(add_memory))
        .route("/metrics", get(get_metrics))
        .route("/auth/password", post(change_password))
        .merge(admin_routes) // Merge admin routes under the main auth middleware
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
    let agent_count = orchestrator.list_agents().await.len();
    let uptime_seconds = state.start_time.elapsed().as_secs();

    // In a real implementation, you would get this from the actual memory system
    let memory_fragments = 0; // Dummy value

    let response = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds,
        agent_count,
        memory_fragments,
    };

    Ok(Json(response))
}

/// List all registered agents
#[instrument(skip(state))]
async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<Vec<AgentInfo>>, StatusCode> {
    let orchestrator = state.orchestrator.read().await;
    let agents = orchestrator.list_agents().await;

    let agent_infos: Vec<AgentInfo> = agents
        .into_iter()
        .map(|(name, agent_type)| AgentInfo {
            name,
            agent_type,
            status: "active".to_string(),
        })
        .collect();

    Ok(Json(agent_infos))
}

use crate::agent::AgentFactory;

/// Register a new agent
#[instrument(skip(state))]
async fn register_agent(
    State(state): State<AppState>,
    Json(request): Json<RegisterAgentRequest>,
) -> Result<StatusCode, StatusCode> {
    let agent = AgentFactory::create_agent(&request.agent_type, request.config, &state.settings)
        .map_err(|e| {
            warn!("Failed to create agent '{}': {}", request.name, e);
            StatusCode::BAD_REQUEST
        })?;

    let mut orchestrator = state.orchestrator.write().await;
    orchestrator.register_agent(request.name.clone(), agent).await.map_err(|e| {
        error!("Failed to register agent '{}': {}", request.name, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("Registered agent: {}", request.name);
    Ok(StatusCode::CREATED)
}

/// Remove an agent
#[instrument(skip(state))]
async fn remove_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let orchestrator = state.orchestrator.read().await;
    orchestrator.remove_agent(&name).await.map_err(|e| {
        error!("Failed to remove agent '{}': {}", name, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    info!("Removed agent: {}", name);
    Ok(StatusCode::NO_CONTENT)
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
    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel(1);

    orchestrator.dispatch((
        request.agent_name.clone(),
        request.input,
        resp_tx,
    )).await?;

    let execution_time = start_time.elapsed().as_millis() as u64;

    match resp_rx.recv().await {
        Some(Ok(result)) => {
            Ok(Json(ExecuteTaskResponse {
                success: true,
                result: Some(result.to_string()),
                error: None,
                execution_time_ms: execution_time,
            }))
        }
        Some(Err(e)) => {
            error!("Task execution failed: {}", e);
            Ok(Json(ExecuteTaskResponse {
                success: false,
                result: None,
                error: Some(e.to_string()),
                execution_time_ms: execution_time,
            }))
        }
        None => {
            error!("Task execution response channel closed unexpectedly");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
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
    let auth_manager = state.auth_manager.clone();
    let username = request.username.clone();
    let password = request.password.clone();

    // Use spawn_blocking for synchronous database operations
    let result = tokio::task::spawn_blocking(move || {
        auth_manager.authenticate(&username, &password)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match result {
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
    let auth_manager = state.auth_manager.clone();

    let result = tokio::task::spawn_blocking(move || {
        auth_manager.add_user(request.username, &request.password, request.roles)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match result {
        Ok(_) => {
            info!("User created successfully");
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            error!("Failed to create user: {}", e);
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
    let auth_manager = state.auth_manager.clone();

    let result = tokio::task::spawn_blocking(move || {
        auth_manager.update_password(&request.username, &request.new_password)
    }).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match result {
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

    // Enforce strict JWT secret validation
    validate_jwt_secret_startup(settings)?;

    // Create application state
    let memory = Arc::new(create_dummy_memory());
    let orchestrator = Arc::new(RwLock::new(
        Orchestrator::new(&settings, memory).await
            .map_err(|e| {
                error!("Failed to initialize orchestrator: {}", e);
                anyhow::anyhow!("Orchestrator initialization failed")
            })?
    ));

    // Initialize authentication manager with validated JWT secret
    let db_path = settings.db_path.clone().unwrap_or_else(|| "./acropolis_db/auth".to_string());
    let jwt_secret = get_jwt_secret_for_server(settings)?;
    let auth_manager = Arc::new(AuthManager::new(jwt_secret, &db_path)?);
    
    // Check admin initialization
    if settings.security.enable_authentication && !auth_manager.has_admin()? {
        error!("No admin user found. Run 'acropolis-cli init-admin' to create the first admin user.");
        return Err(anyhow::anyhow!("Admin user must be initialized before starting the server"));
    }

    // Initialize rate limiter
    let rate_limiter = create_rate_limiter(&settings.security);

    let state = AppState {
        orchestrator,
        auth_manager,
        rate_limiter,
        settings: settings.clone(),
        start_time: std::time::Instant::now(),
    };

    // Create router
    let app = create_router(state);

    // Bind to address
    let addr = format!("{}:{}", settings.server.host, settings.server.port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid server address: {}", e))?;

    info!("HTTP server listening on {}", addr);

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(&addr).await
        .map_err(|e| anyhow::anyhow!("Failed to bind to address: {}", e))?;
    
    let server = axum::serve(listener, app);

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

/// Validate JWT secret meets security requirements for server startup
fn validate_jwt_secret_startup(settings: &Settings) -> Result<()> {
    // First check if JWT secret is required
    if settings.security.enable_authentication {
        let jwt_secret = get_jwt_secret_for_server(settings)?;
        
        // Check minimum length
        if jwt_secret.len() < 32 {
            return Err(anyhow::anyhow!("JWT secret must be at least 32 characters long"));
        }
        
        // Check for default/weak secrets
        let weak_secrets = [
            "default_insecure_secret_change_in_production",
            "secret",
            "jwt_secret", 
            "change_me",
            "insecure",
        ];
        
        if weak_secrets.contains(&jwt_secret.as_str()) {
            return Err(anyhow::anyhow!(
                "JWT secret is using a known weak value. Please set AEP_JWT_SECRET environment variable with a strong, random secret."
            ));
        }
        
        // Basic entropy check
        let unique_chars: std::collections::HashSet<char> = jwt_secret.chars().collect();
        if unique_chars.len() < 4 {
            return Err(anyhow::anyhow!(
                "JWT secret lacks sufficient entropy. Use a random, complex secret."
            ));
        }
    }
    
    Ok(())
}

/// Get JWT secret for server startup
fn get_jwt_secret_for_server(settings: &Settings) -> Result<String> {
    settings.security.jwt_secret.clone()
        .or_else(|| std::env::var("AEP_JWT_SECRET").ok())
        .ok_or_else(|| anyhow::anyhow!(
            "JWT secret must be provided via AEP_JWT_SECRET environment variable or config file when authentication is enabled"
        ))
}
