# Production Deployment Guide

This document provides guidance for deploying MCP servers in production environments, covering rate limiting, scaling, monitoring, and operational best practices.

## Quick Reference

| Concern | Recommendation | See Section |
|---------|----------------|-------------|
| Rate Limiting | Enable for all public endpoints | [Rate Limiting](#rate-limiting) |
| Scaling | Use connection pooling + load balancing | [Scaling Strategies](#scaling-strategies) |
| Monitoring | Export metrics to Prometheus/OpenTelemetry | [Monitoring](#monitoring) |
| Security | Use TLS + OAuth 2.1 | [Security Checklist](#security-checklist) |
| Runtime | Prefer Tokio for production | [Runtime Selection](#runtime-selection) |

## Rate Limiting

**Rate limiting is NOT enabled by default.** You must explicitly configure it to protect against abuse.

### Why Rate Limiting Matters

Without rate limiting, a malicious or misbehaving client can:
- Exhaust server resources with excessive requests
- Cause service degradation for other clients
- Trigger expensive operations repeatedly (database queries, API calls)
- Perform denial-of-service attacks

### Configuration

```rust
use mcpkit_transport::middleware::{RateLimitConfig, RateLimitAlgorithm, RateLimitAction};
use std::time::Duration;

// Production-recommended configuration
let config = RateLimitConfig::new(100, Duration::from_secs(60))
    .with_burst(10)  // Allow small bursts for interactive usage
    .with_algorithm(RateLimitAlgorithm::TokenBucket)
    .with_action(RateLimitAction::Reject);
```

### Algorithm Selection

| Algorithm | Best For | Memory | Precision |
|-----------|----------|--------|-----------|
| `TokenBucket` | General use, allows bursts | O(1) | High |
| `SlidingWindow` | Strict per-window limits | O(n) requests | Highest |
| `FixedWindow` | Simple counting, low memory | O(1) | Lower |

**Recommendation:** Use `TokenBucket` (default) for most deployments.

### Per-Tool Rate Limits

For expensive operations, implement tool-specific limits:

```rust
use std::collections::HashMap;

struct ToolRateLimiter {
    limits: HashMap<String, RateLimiter>,
}

impl ToolRateLimiter {
    fn new() -> Self {
        let mut limits = HashMap::new();

        // Expensive database operations: 10/minute
        limits.insert(
            "query_database".to_string(),
            RateLimiter::new(RateLimitConfig::new(10, Duration::from_secs(60))),
        );

        // Cheap operations: 100/minute
        limits.insert(
            "get_status".to_string(),
            RateLimiter::new(RateLimitConfig::new(100, Duration::from_secs(60))),
        );

        Self { limits }
    }

    async fn check(&self, tool_name: &str) -> Result<(), TransportError> {
        if let Some(limiter) = self.limits.get(tool_name) {
            limiter.check().await
        } else {
            Ok(()) // No limit for unknown tools
        }
    }
}
```

### Monitoring Rate Limits

```rust
// Get rate limiting statistics
let stats = limiter.stats();
println!("Total requests: {}", stats.total_requests);
println!("Rejected: {}", stats.total_rejected);
println!("Rejection rate: {:.2}%", stats.rejection_rate() * 100.0);

// Alert on high rejection rates
if stats.rejection_rate() > 0.1 {
    tracing::warn!(
        rejection_rate = stats.rejection_rate(),
        "High rate limit rejection rate detected"
    );
}
```

## Scaling Strategies

### Horizontal Scaling

MCP servers are stateless by design and can be horizontally scaled behind a load balancer.

```
                    ┌─────────────────────┐
                    │   Load Balancer     │
                    │   (nginx/HAProxy)   │
                    └──────────┬──────────┘
                               │
            ┌──────────────────┼──────────────────┐
            │                  │                  │
     ┌──────┴──────┐    ┌──────┴──────┐    ┌──────┴──────┐
     │ MCP Server  │    │ MCP Server  │    │ MCP Server  │
     │   Pod 1     │    │   Pod 2     │    │   Pod 3     │
     └─────────────┘    └─────────────┘    └─────────────┘
```

#### Load Balancer Configuration (nginx)

```nginx
upstream mcp_servers {
    least_conn;  # Distribute to least loaded server
    server mcp-server-1:8080;
    server mcp-server-2:8080;
    server mcp-server-3:8080;

    keepalive 32;  # Connection pooling
}

server {
    listen 443 ssl http2;
    server_name mcp.example.com;

    # TLS configuration
    ssl_certificate /etc/ssl/certs/mcp.crt;
    ssl_certificate_key /etc/ssl/private/mcp.key;
    ssl_protocols TLSv1.3;

    # WebSocket support
    location /mcp {
        proxy_pass http://mcp_servers;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;

        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }
}
```

### Connection Pooling

For clients connecting to multiple MCP servers:

```rust
use mcpkit_transport::pool::{ConnectionPool, PoolConfig};

let pool = ConnectionPool::new(PoolConfig {
    min_connections: 2,
    max_connections: 10,
    idle_timeout: Duration::from_secs(300),
    connection_timeout: Duration::from_secs(30),
});

// Acquire connection from pool
let transport = pool.acquire().await?;

// Connection automatically returns to pool when dropped
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mcp-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: mcp-server
  template:
    metadata:
      labels:
        app: mcp-server
    spec:
      containers:
      - name: mcp-server
        image: your-registry/mcp-server:latest
        ports:
        - containerPort: 8080
        resources:
          requests:
            memory: "128Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: mcp-server
spec:
  selector:
    app: mcp-server
  ports:
  - port: 8080
    targetPort: 8080
  type: ClusterIP
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: mcp-server-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: mcp-server
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

## Monitoring

### Metrics Export

The SDK provides telemetry support for monitoring:

```rust
use mcpkit_transport::telemetry::{TelemetryConfig, TelemetryLayer};

// Configure telemetry
let telemetry = TelemetryConfig::new()
    .with_service_name("mcp-server")
    .with_metrics_endpoint("http://prometheus:9090/api/v1/write")
    .build();

// Apply to transport
let transport = telemetry.layer(base_transport);
```

### Key Metrics to Monitor

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `mcp_requests_total` | Total requests received | N/A (use rate) |
| `mcp_request_duration_seconds` | Request latency | p99 > 1s |
| `mcp_errors_total` | Error count | Rate > 1/min |
| `mcp_rate_limit_rejections` | Rate limit rejections | Rate > 10/min |
| `mcp_active_connections` | Current connections | > 80% of max |
| `mcp_tool_invocations` | Tool calls by name | Varies |

### Prometheus Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'mcp-server'
    static_configs:
      - targets: ['mcp-server:8080']
    metrics_path: /metrics
    scrape_interval: 15s
```

### Grafana Dashboard

Key panels for MCP monitoring:
1. Request rate and latency (p50, p90, p99)
2. Error rate by type
3. Rate limit rejection rate
4. Active connections over time
5. Tool invocation breakdown
6. Resource access patterns

### Structured Logging

```rust
use tracing_subscriber::{fmt, EnvFilter};

// Production logging setup
tracing_subscriber::fmt()
    .json()  // Structured JSON logs
    .with_env_filter(EnvFilter::from_default_env())
    .with_target(true)
    .with_file(true)
    .with_line_number(true)
    .init();

// Request tracing
tracing::info!(
    request_id = %request.id(),
    method = %request.method(),
    client_id = %context.client_id(),
    "Processing request"
);
```

## Runtime Selection

### Tokio (Recommended for Production)

```toml
[dependencies]
mcpkit = { version = "0.2", features = ["tokio-runtime"] }
```

**Advantages:**
- Most mature and battle-tested
- Best performance for high-throughput scenarios
- Excellent ecosystem integration
- Comprehensive debugging tools

### async-std

> **Note:** async-std is marked as discontinued (RUSTSEC-2025-0052). While still functional, new projects should prefer Tokio.

```toml
[dependencies]
mcpkit = { version = "0.2", features = ["async-std-runtime"] }
```

**Use cases:**
- Existing async-std codebases
- Simple applications with lower throughput requirements

### smol

```toml
[dependencies]
mcpkit = { version = "0.2", features = ["smol-runtime"] }
```

**Use cases:**
- Embedded systems
- Minimal binary size requirements
- Simple threading model

## Security Checklist

Before deploying to production:

### Transport Security
- [ ] TLS enabled for all network transports
- [ ] TLS 1.3 preferred (TLS 1.2 minimum)
- [ ] Certificate from trusted CA (not self-signed)
- [ ] Certificate pinning for high-security deployments

### Authentication
- [ ] OAuth 2.1 implemented with PKCE
- [ ] Token validation on every request
- [ ] Short-lived access tokens (< 1 hour)
- [ ] Refresh token rotation enabled

### Rate Limiting
- [ ] Global rate limits configured
- [ ] Per-tool rate limits for expensive operations
- [ ] Rate limit alerts configured
- [ ] Graceful degradation strategy defined

### Monitoring
- [ ] Metrics exported to monitoring system
- [ ] Alerts configured for errors and latency
- [ ] Structured logging enabled
- [ ] Audit logging for sensitive operations

### Input Validation
- [ ] All tool parameters validated
- [ ] Resource URIs sanitized
- [ ] Path traversal prevention implemented
- [ ] SQL injection prevention (parameterized queries)

## Configuration Management

### Environment Variables

```bash
# Server configuration
MCP_SERVER_HOST=0.0.0.0
MCP_SERVER_PORT=8080
MCP_LOG_LEVEL=info
MCP_LOG_FORMAT=json

# Rate limiting
MCP_RATE_LIMIT_REQUESTS=100
MCP_RATE_LIMIT_WINDOW_SECS=60
MCP_RATE_LIMIT_BURST=10

# TLS
MCP_TLS_CERT_PATH=/etc/ssl/certs/server.crt
MCP_TLS_KEY_PATH=/etc/ssl/private/server.key

# OAuth
MCP_OAUTH_ISSUER=https://auth.example.com
MCP_OAUTH_AUDIENCE=https://mcp.example.com
```

### Configuration Loading

```rust
use std::env;

struct ServerConfig {
    host: String,
    port: u16,
    rate_limit: RateLimitConfig,
}

impl ServerConfig {
    fn from_env() -> Self {
        Self {
            host: env::var("MCP_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("MCP_SERVER_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
            rate_limit: RateLimitConfig::new(
                env::var("MCP_RATE_LIMIT_REQUESTS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(100),
                Duration::from_secs(
                    env::var("MCP_RATE_LIMIT_WINDOW_SECS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60),
                ),
            ),
        }
    }
}
```

## Health Checks

Implement health endpoints for orchestration:

```rust
use axum::{Router, routing::get, Json};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    uptime_secs: u64,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: get_uptime_secs(),
    })
}

#[derive(Serialize)]
struct ReadyResponse {
    ready: bool,
    checks: Vec<CheckResult>,
}

#[derive(Serialize)]
struct CheckResult {
    name: &'static str,
    healthy: bool,
    message: Option<String>,
}

async fn ready(state: State<AppState>) -> Json<ReadyResponse> {
    let checks = vec![
        CheckResult {
            name: "database",
            healthy: state.db.ping().await.is_ok(),
            message: None,
        },
        CheckResult {
            name: "cache",
            healthy: state.cache.ping().await.is_ok(),
            message: None,
        },
    ];

    let ready = checks.iter().all(|c| c.healthy);
    Json(ReadyResponse { ready, checks })
}

// Add to router
let app = Router::new()
    .route("/health", get(health))
    .route("/ready", get(ready));
```

## Graceful Shutdown

Handle shutdown signals properly:

```rust
use tokio::signal;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C"),
        _ = terminate => tracing::info!("Received SIGTERM"),
    }
}

#[tokio::main]
async fn main() {
    // Start server with graceful shutdown
    let server = axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal());

    if let Err(e) = server.await {
        tracing::error!(error = %e, "Server error");
    }
}
```

## Troubleshooting

### Common Issues

| Symptom | Possible Cause | Solution |
|---------|---------------|----------|
| High latency | No connection pooling | Enable pooling |
| Memory growth | No message size limits | Set limits |
| Connection drops | No ping/pong | Enable keepalive |
| Rate limit errors | Limits too strict | Tune limits |
| TLS errors | Certificate issues | Check cert chain |

### Debug Logging

```bash
# Enable debug logging for troubleshooting
RUST_LOG=mcpkit=debug cargo run
```

### Performance Profiling

```bash
# CPU profiling with perf
perf record -g ./target/release/mcp-server
perf report

# Memory profiling with heaptrack
heaptrack ./target/release/mcp-server
heaptrack_gui heaptrack.mcp-server.*.gz
```

## References

- [Security Guide](security.md) - Comprehensive security documentation
- [Performance Guide](performance.md) - Optimization techniques
- [Transports Guide](transports.md) - Transport configuration
- [Middleware Guide](middleware.md) - Rate limiting and other middleware
