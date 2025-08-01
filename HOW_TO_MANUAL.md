# 📚 Adaptive Expert Platform - Complete How-To Manual

## Table of Contents

1. [Quick Start Guide](#-quick-start-guide)
2. [Installation & Setup](#️-installation--setup)
3. [Security Configuration](#-security-configuration)
4. [Plugin Management](#-plugin-management)
5. [Agent Development](#-agent-development)
6. [API Usage](#-api-usage)
7. [Monitoring & Observability](#-monitoring--observability)
8. [Troubleshooting](#-troubleshooting)
9. [Production Deployment](#-production-deployment)
10. [Backup & Recovery](#-backup--recovery)

---

## 🚀 Quick Start Guide

### Prerequisites

- **Rust** 1.70+ with `cargo`
- **Julia** 1.9+ (for Julia agents)
- **Python** 3.8+ (for Python tools)
- **Docker** (optional, for containerized deployment)
- **Redis** (optional, for distributed caching)

### 5-Minute Setup

```bash
# 1. Clone and build
git clone https://github.com/adaptive-expert-platform/core.git
cd core
cargo build --release --all-features

# 2. Generate secure JWT secret
export AEP_SECURITY__JWT_SECRET="$(openssl rand -base64 32)"

# 3. Start the server
./target/release/acropolis-cli serve

# 4. Login (change password immediately!)
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin123"}'
```

---

## ⚙️ Installation & Setup

### From Source

```bash
# Clone repository
git clone https://github.com/adaptive-expert-platform/core.git
cd core

# Install system dependencies
./build.sh  # Installs Julia, Python dependencies

# Build with all features
cargo build --release --all-features

# Install binary
sudo cp target/release/acropolis-cli /usr/local/bin/
```

### Using Docker

```bash
# Pull latest image
docker pull ghcr.io/adaptive-expert-platform/adaptive-expert-platform:latest

# Run with Docker Compose
curl -O https://raw.githubusercontent.com/adaptive-expert-platform/core/main/docker-compose.yml
docker-compose up -d
```

### Pre-built Binaries

Download from [releases page](https://github.com/adaptive-expert-platform/core/releases) and add to PATH.

### Verification

```bash
# Check installation
acropolis-cli --version
acropolis-cli --help

# Health check
curl http://localhost:8080/health
```

---

## 🔐 Security Configuration

### 1. Initial Security Setup

#### **Generate JWT Secret**

```bash
# Generate secure 256-bit secret
JWT_SECRET=$(openssl rand -base64 32)
echo "Generated JWT Secret: $JWT_SECRET"

# Set environment variable
export AEP_SECURITY__JWT_SECRET="$JWT_SECRET"
```

#### **Change Default Admin Password**

```bash
# Login with default credentials
TOKEN=$(curl -s -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "admin", "password": "admin123"}' | \
  jq -r '.token')

# Change password
curl -X POST http://localhost:8080/auth/password \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "new_password": "new-secure-password"
  }'
```

### 2. User Management

#### **Create New Users**

```bash
# Create regular user
curl -X POST http://localhost:8080/auth/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "operator",
    "password": "secure-password",
    "roles": ["user"]
  }'

# Create admin user
curl -X POST http://localhost:8080/auth/users \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin2",
    "password": "secure-password",
    "roles": ["admin", "user"]
  }'
```

#### **Available Roles**

- `user`: Basic access to execute tasks and query agents
- `admin`: Full system access including user management

### 3. Plugin Security

#### **Generate Plugin Hashes**

```bash
# Calculate SHA256 hash for each plugin
sha256sum plugins/dqn_plugin/target/release/libdqn_plugin.so
sha256sum plugins/julia_plugin/target/release/libjulia_plugin.so

# Add to config.toml
[security]
plugin_allowlist_hashes = [
    "a1b2c3d4e5f67890abcdef1234567890abcdef1234567890abcdef1234567890",
    "f6e5d4c3b2a1098765fedc4321098765fedc4321098765fedc4321098765fedc"
]
```

#### **Plugin Development Workflow**

```bash
# 1. Build plugin
cd plugins/my_plugin
cargo build --release

# 2. Calculate hash
PLUGIN_HASH=$(sha256sum target/release/libmy_plugin.so | cut -d' ' -f1)
echo "Plugin hash: $PLUGIN_HASH"

# 3. Add to allowlist in config.toml

# 4. Test plugin loading
acropolis-cli serve --config config.toml
```

### 4. Network Security

#### **CORS Configuration**

```toml
[security]
enable_cors = true
allowed_origins = [
    "https://yourdomain.com",
    "https://app.yourdomain.com"
]
```

#### **Rate Limiting**

```toml
[security]
enable_rate_limiting = true
rate_limit_per_minute = 100  # Adjust based on needs
```

---

## 🔌 Plugin Management

### 1. Built-in Plugins

#### **DQN Plugin** (Reinforcement Learning)

```bash
# Build DQN plugin
cd plugins/dqn_plugin
cargo build --release

# Calculate hash and add to allowlist
sha256sum target/release/libdqn_plugin.so
```

#### **Julia Plugin** (Scientific Computing)

```bash
# Build Julia plugin
cd plugins/julia_plugin
cargo build --release

# Test Julia code execution
curl -X POST http://localhost:8080/execute \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "julia_agent",
    "input": {
      "code": "println(\"Hello from Julia!\")"
    }
  }'
```

### 2. Creating Custom Plugins

#### **Plugin Template**

```rust
// src/lib.rs
use adaptive_expert_platform::{Agent, Memory};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

pub struct MyCustomAgent;

#[async_trait]
impl Agent for MyCustomAgent {
    fn name(&self) -> &str {
        "my_custom_agent"
    }

    fn agent_type(&self) -> &str {
        "custom"
    }

    fn capabilities(&self) -> Vec<String> {
        vec!["custom_processing".to_string()]
    }

    async fn handle(&self, input: Value, _memory: Arc<Memory>) -> Result<String> {
        // Your custom logic here
        Ok(format!("Processed: {}", input))
    }

    async fn health_check(&self) -> Result<crate::agent::AgentHealth> {
        Ok(crate::agent::AgentHealth::default())
    }
}

#[no_mangle]
pub extern "C" fn create_agent() -> *mut dyn Agent {
    Box::into_raw(Box::new(MyCustomAgent))
}
```

#### **Cargo.toml for Plugin**

```toml
[package]
name = "my_custom_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
adaptive_expert_platform = { path = "../../adaptive_expert_platform" }
anyhow = "1.0"
async-trait = "0.1"
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
```

### 3. Plugin Testing

```bash
# Build plugin
cargo build --release

# Test in development mode (signatures disabled)
export AEP_SECURITY__ENABLE_PLUGIN_SIGNATURES=false
acropolis-cli serve

# Test plugin functionality
curl -X POST http://localhost:8080/execute \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "my_custom_agent",
    "input": {"test": "data"}
  }'
```

---

## 🤖 Agent Development

### 1. Julia Agents

#### **Causal Inference Example**

```julia
# models/julia/my_causal_model.jl
using CausalInference
using DataFrames
using CSV

function run_causal_analysis(config_json)
    config = JSON.parse(config_json)

    # Load data
    data = CSV.read(config["data_path"], DataFrame)

    # Run causal inference
    result = pc_algorithm(data, 0.05)

    return JSON.json(Dict(
        "edges" => result.edges,
        "nodes" => result.nodes
    ))
end
```

#### **Using Julia Agent**

```bash
curl -X POST http://localhost:8080/execute \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "julia_agent",
    "input": {
      "code": "using JSON; run_causal_analysis(\"{\\\"data_path\\\": \\\"data/health.csv\\\"}\")"
    }
  }'
```

### 2. Python Agents

#### **Custom Python Script**

```python
# models/python/my_analysis.py
import sys
import json
import pandas as pd

def main():
    # Read input from command line
    input_data = json.loads(sys.argv[1])

    # Process data
    df = pd.read_csv(input_data['file_path'])
    result = df.describe().to_dict()

    # Output result
    print(json.dumps(result))

if __name__ == "__main__":
    main()
```

#### **Using Python Agent**

```bash
curl -X POST http://localhost:8080/execute \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "python_tool",
    "input": {
      "script_path": "models/python/my_analysis.py",
      "args": ["{\"file_path\": \"data/health.csv\"}"]
    }
  }'
```

### 3. Native Rust Agents

Built directly into the platform for maximum performance.

```rust
// Example in adaptive_expert_platform/src/agent.rs
pub struct MyNativeAgent;

#[async_trait]
impl Agent for MyNativeAgent {
    fn name(&self) -> &str { "my_native_agent" }

    async fn handle(&self, input: Value, memory: Arc<Memory>) -> Result<String> {
        // High-performance processing
        Ok("Result".to_string())
    }
}
```

---

## 🌐 API Usage

### 1. Authentication

#### **Login**

```bash
curl -X POST http://localhost:8080/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "password": "your-password"
  }'

# Response:
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "expires_in": 28800,
  "user_id": "admin",
  "roles": ["admin", "user"]
}
```

#### **Using Tokens**

```bash
export TOKEN="your-jwt-token"

# All subsequent requests
curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/agents
```

### 2. Agent Management

#### **List Agents**

```bash
curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/agents

# Response:
[
  {
    "name": "echo",
    "agent_type": "built-in",
    "status": "active"
  },
  {
    "name": "julia_agent",
    "agent_type": "plugin",
    "status": "active"
  }
]
```

#### **Execute Tasks**

```bash
curl -X POST http://localhost:8080/execute \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "echo",
    "input": {"message": "Hello World"},
    "timeout_seconds": 30
  }'

# Response:
{
  "success": true,
  "result": "Echo: Hello World",
  "error": null,
  "execution_time_ms": 5
}
```

### 3. Memory Management

#### **Add Memory**

```bash
curl -X POST http://localhost:8080/memory/add \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Important information to remember"
  }'
```

#### **Search Memory**

```bash
curl -X POST http://localhost:8080/memory/search \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "important information"
  }'
```

### 4. System Monitoring

#### **Health Check**

```bash
curl http://localhost:8080/health

# Response:
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "agent_count": 5,
  "memory_fragments": 100
}
```

#### **Metrics**

```bash
curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/metrics

# Response:
{
  "requests_per_second": 10,
  "average_response_time_ms": 50,
  "error_rate": 0.01,
  "memory_usage_mb": 256,
  "cpu_usage_percent": 15
}
```

---

## 📊 Monitoring & Observability

### 1. Logging Configuration

```toml
[logging]
level = "info"              # debug, info, warn, error
format = "json"             # json or text
output = "stdout"           # stdout, stderr, or file path
enable_timestamps = true
enable_thread_ids = true
enable_target = false
```

### 2. OpenTelemetry Integration

```toml
[observability]
enable_tracing = true
otlp_endpoint = "http://localhost:4317"
jaeger_endpoint = "http://localhost:14268"
tracing_sampler = 0.1       # Sample 10% of traces
```

### 3. Prometheus Metrics

```bash
# Enable metrics endpoint
curl http://localhost:9090/metrics

# Example metrics:
# adaptive_expert_platform_requests_total{method="POST",endpoint="/execute"} 1234
# adaptive_expert_platform_response_time_seconds{endpoint="/execute"} 0.05
# adaptive_expert_platform_errors_total{type="authentication"} 5
```

### 4. Health Monitoring

```bash
# Automated health check script
#!/bin/bash
HEALTH=$(curl -s http://localhost:8080/health | jq -r '.status')
if [ "$HEALTH" != "healthy" ]; then
    echo "ALERT: Platform is unhealthy"
    # Send alert notification
fi
```

---

## 🔧 Troubleshooting

### 1. Common Issues

#### **Authentication Failures**

```bash
# Check JWT secret configuration
echo $AEP_SECURITY__JWT_SECRET

# Verify token validity
curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/agents
```

#### **Plugin Loading Failures**

```bash
# Check plugin hashes
sha256sum plugins/*/target/release/*.so

# Verify allowlist configuration
grep -A 10 "plugin_allowlist_hashes" config.toml

# Check plugin signature verification
export AEP_SECURITY__ENABLE_PLUGIN_SIGNATURES=false  # Development only
```

#### **Julia Runtime Issues**

```bash
# Check Julia installation
julia --version

# Test Julia runtime
julia -e "println(\"Julia is working\")"

# Check Julia plugin build
cd plugins/julia_plugin
cargo build --release
```

#### **Memory Issues**

```bash
# Check memory usage
curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/memory/stats

# Clear memory cache (if using Redis)
redis-cli FLUSHDB
```

### 2. Debug Mode

```bash
# Enable debug logging
export RUST_LOG=debug
export AEP_LOGGING__LEVEL=debug

# Start with verbose output
acropolis-cli serve --config config.toml
```

### 3. Performance Issues

```bash
# Check resource usage
top -p $(pgrep acropolis-cli)

# Monitor request latency
curl -w "@curl-format.txt" -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/agents

# curl-format.txt:
#     time_namelookup:  %{time_namelookup}\n
#     time_connect:     %{time_connect}\n
#     time_appconnect:  %{time_appconnect}\n
#     time_pretransfer: %{time_pretransfer}\n
#     time_redirect:    %{time_redirect}\n
#     time_starttransfer: %{time_starttransfer}\n
#     ----------\n
#     time_total:       %{time_total}\n
```

---

## 🚢 Production Deployment

### 1. Docker Deployment

#### **docker-compose.yml**

```yaml
version: '3.8'
services:
  acropolis:
    image: ghcr.io/adaptive-expert-platform/adaptive-expert-platform:latest
    ports:
      - "8080:8080"
      - "9090:9090"  # Metrics
    environment:
      - AEP_SECURITY__JWT_SECRET=${JWT_SECRET}
      - AEP_SECURITY__ENABLE_AUTHENTICATION=true
      - AEP_SERVER__HOST=0.0.0.0
      - AEP_MEMORY__PROVIDER=redis
      - AEP_MEMORY__URL=redis://redis:6379
    volumes:
      - ./config.toml:/app/config.toml:ro
      - ./plugins:/app/plugins:ro
      - ./models:/app/models:ro
    depends_on:
      - redis
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  redis:
    image: redis:7-alpine
    volumes:
      - redis_data:/data
    restart: unless-stopped
    command: redis-server --appendonly yes

volumes:
  redis_data:
```

#### **Deployment Commands**

```bash
# Generate production secrets
export JWT_SECRET=$(openssl rand -base64 32)

# Deploy
docker-compose up -d

# Check health
docker-compose ps
curl http://localhost:8080/health
```

### 2. Kubernetes Deployment

#### **ConfigMap**

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: acropolis-config
data:
  config.toml: |
    [server]
    host = "0.0.0.0"
    port = 8080

    [security]
    enable_authentication = true
    # JWT secret will be provided via secret
```

#### **Secret**

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: acropolis-secret
type: Opaque
data:
  jwt-secret: <base64-encoded-jwt-secret>
```

#### **Deployment**

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: acropolis
spec:
  replicas: 3
  selector:
    matchLabels:
      app: acropolis
  template:
    metadata:
      labels:
        app: acropolis
    spec:
      containers:
      - name: acropolis
        image: ghcr.io/adaptive-expert-platform/adaptive-expert-platform:latest
        ports:
        - containerPort: 8080
        - containerPort: 9090
        env:
        - name: AEP_SECURITY__JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: acropolis-secret
              key: jwt-secret
        - name: AEP_SECURITY__ENABLE_AUTHENTICATION
          value: "true"
        volumeMounts:
        - name: config
          mountPath: /app/config.toml
          subPath: config.toml
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
      volumes:
      - name: config
        configMap:
          name: acropolis-config
```

### 3. Reverse Proxy Setup

#### **Nginx Configuration**

```nginx
server {
    listen 443 ssl http2;
    server_name api.yourdomain.com;

    ssl_certificate /path/to/certificate.crt;
    ssl_certificate_key /path/to/private.key;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options DENY always;
    add_header X-Content-Type-Options nosniff always;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api:10m rate=100r/m;
    limit_req zone=api burst=20 nodelay;

    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeouts
        proxy_connect_timeout 10s;
        proxy_send_timeout 30s;
        proxy_read_timeout 30s;
    }

    location /metrics {
        proxy_pass http://localhost:9090;
        # Restrict access to metrics
        allow 10.0.0.0/8;
        deny all;
    }
}
```

---

## 💾 Backup & Recovery

### 1. Data Backup

#### **Configuration Backup**

```bash
#!/bin/bash
# backup-config.sh
BACKUP_DIR="/backups/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Backup configuration
cp config.toml "$BACKUP_DIR/"
cp -r plugins/ "$BACKUP_DIR/"
cp -r models/ "$BACKUP_DIR/"

# Backup user data (if using file-based storage)
cp -r data/ "$BACKUP_DIR/"

echo "Backup completed: $BACKUP_DIR"
```

#### **Redis Backup**

```bash
# Automated Redis backup
#!/bin/bash
BACKUP_DIR="/backups/redis/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Create Redis backup
redis-cli BGSAVE
sleep 10  # Wait for backup completion
cp /var/lib/redis/dump.rdb "$BACKUP_DIR/"

echo "Redis backup completed: $BACKUP_DIR"
```

### 2. Disaster Recovery

#### **Recovery Procedure**

```bash
#!/bin/bash
# recover.sh
BACKUP_DIR="$1"

if [ -z "$BACKUP_DIR" ]; then
    echo "Usage: $0 <backup_directory>"
    exit 1
fi

# Stop services
docker-compose down

# Restore configuration
cp "$BACKUP_DIR/config.toml" ./
cp -r "$BACKUP_DIR/plugins/" ./
cp -r "$BACKUP_DIR/models/" ./

# Restore Redis data
cp "$BACKUP_DIR/dump.rdb" /var/lib/redis/

# Restart services
docker-compose up -d

echo "Recovery completed from: $BACKUP_DIR"
```

### 3. Automated Backup

#### **Cron Job Setup**

```bash
# Add to crontab (crontab -e)
# Backup every day at 2 AM
0 2 * * * /path/to/backup-config.sh
0 3 * * * /path/to/backup-redis.sh

# Weekly configuration backup
0 4 * * 0 /path/to/full-backup.sh
```

---

## 📞 Support & Resources

### Documentation

- **API Reference**: [docs.adaptive-expert-platform.dev/api](https://docs.adaptive-expert-platform.dev/api)
- **Architecture Guide**: [ARCHITECTURE.md](ARCHITECTURE.md)
- **Security Guide**: [README.md#security](README.md#security)

### Community

- **Discord**: [discord.gg/adaptive-expert-platform](https://discord.gg/adaptive-expert-platform)
- **GitHub Issues**: [github.com/adaptive-expert-platform/core/issues](https://github.com/adaptive-expert-platform/core/issues)
- **Discussions**: [github.com/adaptive-expert-platform/core/discussions](https://github.com/adaptive-expert-platform/core/discussions)

### Professional Support

- **Enterprise Support**: `enterprise@adaptive-expert-platform.dev`
- **Security Issues**: `security@adaptive-expert-platform.dev`
- **Training & Consulting**: `consulting@adaptive-expert-platform.dev`

---

**🎉 You're now ready to use the Adaptive Expert Platform securely and effectively!**
