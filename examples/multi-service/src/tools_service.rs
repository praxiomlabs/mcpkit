//! Tools Service - Provides calculation and utility tools.
//!
//! This service demonstrates a backend MCP server focused on providing tools.
//! In a microservices architecture, this could be one of many specialized
//! services that the gateway aggregates.
//!
//! # Running
//!
//! ```bash
//! cargo run -p multi-service-example --bin tools-service
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
    types::{CallToolResult, Tool, ToolOutput},
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
            server_info: ServerInfo::new("tools-service", "1.0.0"),
            capabilities: ServerCapabilities::new().with_tools(),
        }
    }
}

/// Available tools.
fn get_tools() -> Vec<Tool> {
    vec![
        Tool::new("calculate")
            .description("Perform mathematical calculations")
            .with_string_param("expression", "Math expression to evaluate", true),
        Tool::new("hash")
            .description("Generate a hash of the input")
            .with_string_param("input", "Text to hash", true)
            .with_string_param("algorithm", "Hash algorithm (sha256, md5)", false),
        Tool::new("uuid")
            .description("Generate a new UUID"),
        Tool::new("timestamp")
            .description("Get current timestamp")
            .with_string_param("format", "Output format (unix, iso, rfc2822)", false),
    ]
}

/// Execute a tool.
fn call_tool(name: &str, args: &Value) -> Result<ToolOutput, String> {
    match name {
        "calculate" => {
            let expr = args["expression"]
                .as_str()
                .ok_or("Missing 'expression' parameter")?;
            // Simple calculator - in production, use a proper expression parser
            let result = match expr.trim() {
                "1+1" => "2",
                "2*3" => "6",
                "10/2" => "5",
                _ => return Err(format!("Cannot evaluate: {expr}")),
            };
            Ok(ToolOutput::text(result))
        }
        "hash" => {
            let input = args["input"]
                .as_str()
                .ok_or("Missing 'input' parameter")?;
            let algorithm = args["algorithm"].as_str().unwrap_or("sha256");
            // Simple hash simulation
            let hash = format!("{algorithm}:{:x}", input.len() * 31337);
            Ok(ToolOutput::text(hash))
        }
        "uuid" => {
            let id = uuid::Uuid::new_v4();
            Ok(ToolOutput::text(id.to_string()))
        }
        "timestamp" => {
            let format = args["format"].as_str().unwrap_or("unix");
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let result = match format {
                "unix" => now.to_string(),
                "iso" => format!("1970-01-01T00:00:{}Z", now % 60),
                _ => now.to_string(),
            };
            Ok(ToolOutput::text(result))
        }
        _ => Err(format!("Unknown tool: {name}")),
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
        "tools/list" => {
            let tools: Vec<Value> = get_tools()
                .into_iter()
                .map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                }))
                .collect();
            JsonRpcResponse::success(request.id.clone(), json!({ "tools": tools }))
        }
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args) {
                Ok(output) => {
                    let result: CallToolResult = output.into();
                    JsonRpcResponse::success(
                        request.id.clone(),
                        serde_json::to_value(result).unwrap(),
                    )
                }
                Err(e) => JsonRpcResponse::success(
                    request.id.clone(),
                    json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
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
    common::init_tracing("tools_service");

    let state = Arc::new(AppState::default());
    let addr = format!("0.0.0.0:{}", common::ports::TOOLS);

    let app = Router::new()
        .route("/mcp", post(handle_mcp_post))
        .route("/health", get(health_check))
        .with_state(state);

    info!(addr = %addr, "Starting Tools Service");
    println!("Tools Service running at http://{addr}/mcp");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
