# Extension Development Guide

This guide explains how to build extensions that integrate the mcpkit with web frameworks like Axum, Actix-web, and others.

## Official Extension Crates

The SDK provides official extension crates for popular web frameworks:

| Crate | Framework | Features |
|-------|-----------|----------|
| `mcpkit-axum` | [Axum](https://github.com/tokio-rs/axum) | McpRouter, session management, SSE streaming, CORS support |
| `mcpkit-actix` | [Actix-web](https://actix.rs/) | McpConfig, session management, SSE streaming |

These crates serve as reference implementations and can be used directly or as patterns for custom integrations.

### Using Official Extensions

```toml
# For Axum
mcpkit-axum = "0.1"

# For Actix-web
mcpkit-actix = "0.1"
```

## Overview

Extensions bridge the MCP SDK with specific web frameworks, providing:

- HTTP transport integration
- Session management
- SSE streaming support
- Framework-specific middleware

## Architecture Patterns

### Pattern 1: Transport Adapter

Wrap the SDK's transport layer to work with your framework's I/O primitives.

```rust
use mcpkit_transport::Transport;
use mcpkit_core::protocol::Message;

/// Adapter that bridges framework-specific I/O with MCP transports.
pub struct FrameworkTransportAdapter<T> {
    inner: T,
    // Framework-specific state
}

impl<T: Transport> FrameworkTransportAdapter<T> {
    pub fn new(transport: T) -> Self {
        Self { inner: transport }
    }

    /// Convert framework request to MCP message
    pub async fn handle_request(&self, body: &str) -> Result<String, TransportError> {
        let msg: Message = serde_json::from_str(body)?;

        if let Message::Request(request) = msg {
            // Forward to transport and get response
            self.inner.send(Message::Request(request)).await?;
            if let Some(Message::Response(response)) = self.inner.recv().await? {
                return Ok(serde_json::to_string(&response)?);
            }
        }

        Err(TransportError::Protocol("Invalid message type".into()))
    }
}
```

### Pattern 2: Handler Integration

Integrate directly with MCP handlers for maximum control.

```rust
use mcpkit_server::{ServerHandler, ToolHandler, Context};
use mcpkit_core::types::{Tool, ToolOutput};

/// Extension that wraps an MCP handler for framework integration.
pub struct McpExtension<H> {
    handler: H,
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
}

impl<H: ServerHandler + ToolHandler> McpExtension<H> {
    pub fn new(handler: H) -> Self {
        Self {
            server_info: handler.server_info(),
            capabilities: handler.capabilities(),
            handler,
        }
    }

    /// Process a JSON-RPC request and return a response.
    pub async fn process_request(&self, request: Request) -> Response {
        let ctx = self.create_context(&request);

        match request.method.as_str() {
            "initialize" => self.handle_initialize(&request),
            "tools/list" => self.handle_list_tools(&ctx).await,
            "tools/call" => self.handle_call_tool(&request, &ctx).await,
            // ... other methods
            _ => Response::error(request.id, method_not_found()),
        }
    }

    async fn handle_list_tools(&self, ctx: &Context<'_>) -> Response {
        match self.handler.list_tools(ctx).await {
            Ok(tools) => Response::success(/* ... */),
            Err(e) => Response::error(/* ... */),
        }
    }
}
```

## Axum Extension

### Basic Integration

```rust
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use mcpkit_core::protocol::{Message, Request, Response as JsonRpcResponse};
use std::sync::Arc;

/// Shared state for the MCP extension.
#[derive(Clone)]
pub struct McpState<H> {
    handler: Arc<H>,
    server_info: ServerInfo,
}

impl<H: ServerHandler + Send + Sync + 'static> McpState<H> {
    pub fn new(handler: H) -> Self {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            server_info,
        }
    }
}

/// Handler for MCP POST requests.
pub async fn handle_mcp<H>(
    State(state): State<McpState<H>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse
where
    H: ServerHandler + ToolHandler + Send + Sync + 'static,
{
    // Validate protocol version
    let version = headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok());

    if !is_supported_version(version) {
        return (StatusCode::BAD_REQUEST, "Unsupported protocol version").into_response();
    }

    // Parse and handle message
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    match msg {
        Message::Request(request) => {
            let response = state.handler.handle_request(request).await;
            let body = serde_json::to_string(&Message::Response(response)).unwrap();
            (StatusCode::OK, body).into_response()
        }
        Message::Notification(_) => StatusCode::ACCEPTED.into_response(),
        _ => (StatusCode::BAD_REQUEST, "Unexpected message type").into_response(),
    }
}

/// Create Axum router for MCP.
pub fn mcp_router<H>(handler: H) -> Router
where
    H: ServerHandler + ToolHandler + Send + Sync + Clone + 'static,
{
    Router::new()
        .route("/mcp", post(handle_mcp::<H>))
        .with_state(McpState::new(handler))
}
```

### SSE Streaming

```rust
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;

/// Session manager for SSE connections.
pub struct SessionManager {
    sessions: DashMap<String, broadcast::Sender<String>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub fn create_session(&self) -> (String, broadcast::Receiver<String>) {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = broadcast::channel(100);
        self.sessions.insert(id.clone(), tx);
        (id, rx)
    }

    pub fn send_to_session(&self, id: &str, message: String) -> Result<(), broadcast::error::SendError<String>> {
        if let Some(tx) = self.sessions.get(id) {
            tx.send(message)?;
        }
        Ok(())
    }
}

/// SSE endpoint handler.
pub async fn handle_sse(
    State(sessions): State<Arc<SessionManager>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let (id, mut rx) = match session_id {
        Some(id) => {
            // Existing session
            let rx = sessions.get_receiver(&id)?;
            (id, rx)
        }
        None => sessions.create_session(),
    };

    let stream = async_stream::stream! {
        yield Ok::<_, Infallible>(Event::default().event("connected").data(&id));

        while let Ok(msg) = rx.recv().await {
            yield Ok(Event::default().event("message").data(msg));
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

## Actix-web Extension

### Basic Integration

```rust
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use mcpkit_core::protocol::Message;

/// Shared MCP state for Actix-web.
pub struct McpData<H> {
    handler: H,
}

/// Handler for MCP requests.
pub async fn handle_mcp_request<H>(
    req: HttpRequest,
    body: String,
    data: web::Data<McpData<H>>,
) -> impl Responder
where
    H: ServerHandler + ToolHandler + Send + Sync + 'static,
{
    // Check protocol version
    let version = req
        .headers()
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok());

    if !is_supported_version(version) {
        return HttpResponse::BadRequest().body("Unsupported protocol version");
    }

    // Parse message
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    // Process
    match msg {
        Message::Request(request) => {
            let response = data.handler.handle_request(request).await;
            HttpResponse::Ok()
                .content_type("application/json")
                .body(serde_json::to_string(&Message::Response(response)).unwrap())
        }
        Message::Notification(_) => HttpResponse::Accepted().finish(),
        _ => HttpResponse::BadRequest().body("Unexpected message type"),
    }
}

/// Configure Actix-web app with MCP routes.
pub fn configure_mcp<H>(cfg: &mut web::ServiceConfig, handler: H)
where
    H: ServerHandler + ToolHandler + Send + Sync + Clone + 'static,
{
    cfg.app_data(web::Data::new(McpData { handler }))
        .route("/mcp", web::post().to(handle_mcp_request::<H>));
}
```

## Session Management

### Thread-Safe Session Store

```rust
use dashmap::DashMap;
use std::time::{Duration, Instant};

/// Session with metadata.
pub struct Session {
    pub id: String,
    pub created_at: Instant,
    pub last_active: Instant,
    pub initialized: bool,
    pub client_capabilities: Option<ClientCapabilities>,
}

/// Thread-safe session store with automatic cleanup.
pub struct SessionStore {
    sessions: DashMap<String, Session>,
    timeout: Duration,
}

impl SessionStore {
    pub fn new(timeout: Duration) -> Self {
        Self {
            sessions: DashMap::new(),
            timeout,
        }
    }

    pub fn create(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Instant::now();
        self.sessions.insert(
            id.clone(),
            Session {
                id: id.clone(),
                created_at: now,
                last_active: now,
                initialized: false,
                client_capabilities: None,
            },
        );
        id
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        self.sessions.get(id).map(|r| r.clone())
    }

    pub fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.last_active = Instant::now();
        }
    }

    pub fn cleanup_expired(&self) {
        self.sessions.retain(|_, s| s.last_active.elapsed() < self.timeout);
    }

    pub fn remove(&self, id: &str) -> Option<Session> {
        self.sessions.remove(id).map(|(_, s)| s)
    }
}
```

## Error Handling

### Extension-Specific Errors

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExtensionError {
    #[error("Protocol version {0} is not supported")]
    UnsupportedVersion(String),

    #[error("Session {0} not found")]
    SessionNotFound(String),

    #[error("Session {0} has expired")]
    SessionExpired(String),

    #[error("Invalid JSON-RPC message: {0}")]
    InvalidMessage(String),

    #[error("Handler error: {0}")]
    Handler(#[from] McpError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl ExtensionError {
    /// Convert to HTTP status code.
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::UnsupportedVersion(_) => StatusCode::BAD_REQUEST,
            Self::SessionNotFound(_) => StatusCode::NOT_FOUND,
            Self::SessionExpired(_) => StatusCode::GONE,
            Self::InvalidMessage(_) => StatusCode::BAD_REQUEST,
            Self::Handler(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Serialization(_) => StatusCode::BAD_REQUEST,
        }
    }
}
```

## Testing Extensions

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_testing::MockHandler;

    #[tokio::test]
    async fn test_initialize() {
        let handler = MockHandler::new();
        let ext = McpExtension::new(handler);

        let request = Request::new("initialize", RequestId::Number(1))
            .with_params(json!({
                "protocolVersion": "2025-06-18",
                "clientInfo": {"name": "test", "version": "1.0"}
            }));

        let response = ext.process_request(request).await;
        assert!(response.result.is_some());
    }

    #[tokio::test]
    async fn test_tool_call() {
        let mut handler = MockHandler::new();
        handler.add_tool(Tool::new("test").description("Test tool"));
        handler.on_tool_call("test", |_| Ok(ToolOutput::text("result")));

        let ext = McpExtension::new(handler);
        let request = Request::new("tools/call", RequestId::Number(1))
            .with_params(json!({
                "name": "test",
                "arguments": {}
            }));

        let response = ext.process_request(request).await;
        assert!(response.result.is_some());
    }
}
```

### Integration Testing

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_mcp_endpoint() {
        let app = mcp_router(TestHandler::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("mcp-protocol-version", "2025-06-18")
                    .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
```

## Best Practices

### 1. Protocol Version Validation

Always validate the `mcp-protocol-version` header:

```rust
const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

fn is_supported_version(version: Option<&str>) -> bool {
    version.map(|v| SUPPORTED_VERSIONS.contains(&v)).unwrap_or(false)
}
```

### 2. Graceful Shutdown

Handle server shutdown gracefully:

```rust
pub async fn run_server(app: Router, shutdown_rx: oneshot::Receiver<()>) {
    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
        })
        .await
        .unwrap();
}
```

### 3. Request Tracing

Add request tracing for observability:

```rust
use tracing::{info_span, Instrument};

pub async fn handle_mcp_traced<H>(
    State(state): State<McpState<H>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let span = info_span!(
        "mcp_request",
        session_id = %session_id,
    );

    handle_mcp_inner(state, headers, body)
        .instrument(span)
        .await
}
```

### 4. Rate Limiting

Integrate with framework-specific rate limiting:

```rust
use tower_governor::{Governor, GovernorConfig, GovernorLayer};

pub fn rate_limited_router<H>(handler: H, requests_per_second: u64) -> Router
where
    H: ServerHandler + ToolHandler + Send + Sync + Clone + 'static,
{
    let governor_conf = Box::new(
        GovernorConfig::default()
            .with_rate_limit(requests_per_second),
    );

    Router::new()
        .route("/mcp", post(handle_mcp::<H>))
        .layer(GovernorLayer { config: governor_conf })
        .with_state(McpState::new(handler))
}
```

## Publishing Your Extension

When publishing an extension crate:

1. **Name it appropriately**: `mcpkit-axum`, `mcpkit-actix`, etc.
2. **Depend on stable versions**: Use the published crates.io versions
3. **Document compatibility**: State which SDK and framework versions are supported
4. **Include examples**: Provide runnable examples in `examples/`
5. **Test thoroughly**: Include both unit and integration tests

### Cargo.toml Example

```toml
[package]
name = "mcpkit-axum"
version = "0.1.0"
edition = "2021"
description = "Axum integration for mcpkit"
keywords = ["mcp", "axum", "web", "api"]
categories = ["web-programming", "asynchronous"]

[dependencies]
mcpkit-core = "0.1"
mcpkit-server = "0.1"
axum = "0.7"
tokio = { version = "1", features = ["sync"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
tower = "0.4"
```

## See Also

- [Architecture](architecture.md) - SDK architecture overview
- [Middleware](middleware.md) - Transport middleware patterns
- [Transports](transports.md) - Built-in transport options
- [HTTP Server Example](../examples/http-server) - Complete Axum example
