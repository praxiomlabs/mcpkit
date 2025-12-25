//! Resources Service - Provides data resources.
//!
//! This service demonstrates a backend MCP server focused on providing resources.
//! Resources could be documents, configuration, or any data the client needs.
//!
//! # Running
//!
//! ```bash
//! cargo run -p multi-service-example --bin resources-service
//! ```

mod common;

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use mcpkit_core::{
    capability::{ServerCapabilities, ServerInfo},
    error::JsonRpcError,
    protocol::{Message, Request, Response as JsonRpcResponse},
    types::{Resource, ResourceContents},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::info;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Application state.
#[derive(Clone)]
struct AppState {
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            server_info: ServerInfo::new("resources-service", "1.0.0"),
            capabilities: ServerCapabilities::new().with_resources(),
        }
    }
}

/// Available resources.
fn get_resources() -> Vec<Resource> {
    vec![
        Resource::new("config://app", "Application Configuration")
            .mime_type("application/json")
            .description("Application configuration settings"),
        Resource::new("config://database", "Database Configuration")
            .mime_type("application/json")
            .description("Database connection settings"),
        Resource::new("docs://readme", "README Documentation")
            .mime_type("text/markdown")
            .description("Project documentation"),
        Resource::new("metrics://system", "System Metrics")
            .mime_type("application/json")
            .description("Current system metrics"),
    ]
}

/// Read a resource.
fn read_resource(uri: &str) -> Result<ResourceContents, String> {
    match uri {
        "config://app" => Ok(ResourceContents::text(
            uri,
            serde_json::to_string_pretty(&json!({
                "name": "multi-service-example",
                "version": "1.0.0",
                "environment": "development",
                "features": {
                    "authentication": true,
                    "caching": true,
                    "logging": true,
                }
            }))
            .unwrap(),
        )),
        "config://database" => Ok(ResourceContents::text(
            uri,
            serde_json::to_string_pretty(&json!({
                "host": "localhost",
                "port": 5432,
                "database": "mcpkit",
                "pool_size": 10,
                "ssl": false,
            }))
            .unwrap(),
        )),
        "docs://readme" => Ok(ResourceContents::text(
            uri,
            r#"# Multi-Service MCP Example

This example demonstrates a microservices architecture with MCP.

## Services

- **Gateway**: Aggregates tools and resources from backend services
- **Tools Service**: Provides calculation and utility tools
- **Resources Service**: Provides configuration and documentation

## Architecture

```
Client → Gateway → Tools Service
                 → Resources Service
```

The gateway proxies requests to appropriate backend services.
"#
            .to_string(),
        )),
        "metrics://system" => {
            let uptime = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                % 10000;
            Ok(ResourceContents::text(
                uri,
                serde_json::to_string_pretty(&json!({
                    "uptime_seconds": uptime,
                    "memory_mb": 128,
                    "cpu_percent": 5.2,
                    "connections": 3,
                }))
                .unwrap(),
            ))
        }
        _ => Err(format!("Unknown resource: {uri}")),
    }
}

/// Handle JSON-RPC request.
async fn handle_request(state: &AppState, request: &Request) -> JsonRpcResponse {
    let method = request.method.as_ref();
    let params = request.params.clone().unwrap_or(Value::Null);

    match method {
        "initialize" => JsonRpcResponse::success(
            request.id.clone(),
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": state.server_info,
                "capabilities": state.capabilities,
            }),
        ),
        "resources/list" => {
            let resources: Vec<Value> = get_resources()
                .into_iter()
                .map(|r| json!({
                    "uri": r.uri,
                    "name": r.name,
                    "description": r.description,
                    "mimeType": r.mime_type,
                }))
                .collect();
            JsonRpcResponse::success(request.id.clone(), json!({ "resources": resources }))
        }
        "resources/read" => {
            let uri = params["uri"].as_str().unwrap_or("");
            match read_resource(uri) {
                Ok(contents) => JsonRpcResponse::success(
                    request.id.clone(),
                    json!({
                        "contents": [{
                            "uri": contents.uri,
                            "mimeType": contents.mime_type,
                            "text": contents.text,
                        }],
                    }),
                ),
                Err(e) => JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError {
                        code: -32602,
                        message: e,
                        data: None,
                    },
                ),
            }
        }
        "ping" => JsonRpcResponse::success(request.id.clone(), json!({})),
        _ => JsonRpcResponse::error(
            request.id.clone(),
            JsonRpcError::method_not_found(format!("Method not found: {method}")),
        ),
    }
}

/// Handle MCP POST requests.
async fn handle_mcp_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let version = headers
        .get(MCP_PROTOCOL_VERSION_HEADER)
        .and_then(|v| v.to_str().ok());

    if version != Some(MCP_PROTOCOL_VERSION) {
        return (StatusCode::BAD_REQUEST, "Invalid protocol version").into_response();
    }

    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("Parse error: {e}")).into_response(),
    };

    match msg {
        Message::Request(request) => {
            let response = handle_request(&state, &request).await;
            let body = serde_json::to_string(&Message::Response(response)).unwrap();
            (
                StatusCode::OK,
                [(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)],
                body,
            )
                .into_response()
        }
        Message::Notification(_) => (StatusCode::ACCEPTED, "").into_response(),
        _ => (StatusCode::BAD_REQUEST, "Unexpected message").into_response(),
    }
}

/// Health check.
async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    common::init_tracing("resources_service");

    let state = Arc::new(AppState::default());
    let addr = format!("0.0.0.0:{}", common::ports::RESOURCES);

    let app = Router::new()
        .route("/mcp", post(handle_mcp_post))
        .route("/health", get(health_check))
        .with_state(state);

    info!(addr = %addr, "Starting Resources Service");
    println!("Resources Service running at http://{addr}/mcp");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
