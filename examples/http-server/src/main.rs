//! MCP HTTP Server Example
//!
//! This example demonstrates a full MCP server using HTTP transport with
//! Server-Sent Events (SSE) for streaming responses.
//!
//! # Running
//!
//! ```bash
//! cargo run -p http-server-example
//! ```
//!
//! The server will listen on `http://127.0.0.1:3000/mcp`.
//!
//! # Testing with curl
//!
//! Initialize a session:
//! ```bash
//! curl -X POST http://127.0.0.1:3000/mcp \
//!   -H "Content-Type: application/json" \
//!   -H "mcp-protocol-version: 2025-06-18" \
//!   -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"curl","version":"1.0"}}}'
//! ```

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post},
};
use mcpkit_core::{
    capability::{ClientCapabilities, ServerCapabilities, ServerInfo},
    error::JsonRpcError,
    protocol::{Message, Request, Response as JsonRpcResponse},
    types::{
        CallToolResult, GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput,
    },
};
use serde_json::{Value, json};
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn};

/// MCP Protocol version.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Session ID header name (lowercase for HTTP/2 compatibility).
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Protocol version header name (lowercase for HTTP/2 compatibility).
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Session state.
#[derive(Debug)]
#[allow(dead_code)]
struct Session {
    /// Session ID.
    id: String,
    /// Whether the session is initialized.
    initialized: bool,
    /// Client capabilities.
    client_capabilities: Option<ClientCapabilities>,
    /// Channel to send SSE events.
    sse_tx: broadcast::Sender<String>,
}

/// Application state shared between handlers.
#[derive(Clone)]
#[allow(dead_code)]
struct AppState {
    /// Active sessions.
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    /// Server info.
    server_info: ServerInfo,
    /// Server capabilities.
    server_capabilities: ServerCapabilities,
}

impl AppState {
    fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            server_info: ServerInfo::new("http-mcp-server", "1.0.0"),
            server_capabilities: ServerCapabilities::new()
                .with_tools()
                .with_resources()
                .with_prompts(),
        }
    }

    async fn get_or_create_session(&self, session_id: Option<&str>) -> (String, bool) {
        let mut sessions = self.sessions.write().await;

        if let Some(id) = session_id
            && sessions.contains_key(id)
        {
            return (id.to_string(), false);
        }

        // Create new session
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, _) = broadcast::channel(100);
        sessions.insert(
            id.clone(),
            Session {
                id: id.clone(),
                initialized: false,
                client_capabilities: None,
                sse_tx: tx,
            },
        );

        (id, true)
    }

    async fn mark_initialized(&self, session_id: &str, client_caps: ClientCapabilities) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.initialized = true;
            session.client_capabilities = Some(client_caps);
        }
    }

    async fn remove_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }

    async fn get_sse_receiver(&self, session_id: &str) -> Option<broadcast::Receiver<String>> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).map(|s| s.sse_tx.subscribe())
    }
}

/// Tool definitions.
fn get_tools() -> Vec<Tool> {
    vec![
        Tool::new("add")
            .description("Add two numbers together")
            .with_number_param("a", "First number", true)
            .with_number_param("b", "Second number", true),
        Tool::new("subtract")
            .description("Subtract b from a")
            .with_number_param("a", "First number", true)
            .with_number_param("b", "Second number", true),
        Tool::new("multiply")
            .description("Multiply two numbers")
            .with_number_param("a", "First number", true)
            .with_number_param("b", "Second number", true),
        Tool::new("divide")
            .description("Divide a by b")
            .with_number_param("a", "Dividend", true)
            .with_number_param("b", "Divisor (must not be zero)", true),
        Tool::new("echo")
            .description("Echo back the input message")
            .with_string_param("message", "Message to echo", true),
        Tool::new("get_time").description("Get the current server time"),
    ]
}

/// Resource definitions.
fn get_resources() -> Vec<Resource> {
    vec![
        Resource::new("server://info", "Server Information")
            .mime_type("application/json")
            .description("Information about this MCP server"),
        Resource::new("server://status", "Server Status")
            .mime_type("application/json")
            .description("Current server status and statistics"),
    ]
}

/// Prompt definitions.
fn get_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new("calculator")
            .description("A helpful calculator assistant")
            .optional_arg("operation", "The operation to explain"),
        Prompt::new("greeting")
            .description("Generate a greeting")
            .required_arg("name", "Name to greet"),
    ]
}

/// Handle a tool call.
fn call_tool(name: &str, args: &Value) -> Result<ToolOutput, String> {
    match name {
        "add" => {
            let a = args["a"].as_f64().ok_or("Missing parameter 'a'")?;
            let b = args["b"].as_f64().ok_or("Missing parameter 'b'")?;
            Ok(ToolOutput::text(format!("{}", a + b)))
        }
        "subtract" => {
            let a = args["a"].as_f64().ok_or("Missing parameter 'a'")?;
            let b = args["b"].as_f64().ok_or("Missing parameter 'b'")?;
            Ok(ToolOutput::text(format!("{}", a - b)))
        }
        "multiply" => {
            let a = args["a"].as_f64().ok_or("Missing parameter 'a'")?;
            let b = args["b"].as_f64().ok_or("Missing parameter 'b'")?;
            Ok(ToolOutput::text(format!("{}", a * b)))
        }
        "divide" => {
            let a = args["a"].as_f64().ok_or("Missing parameter 'a'")?;
            let b = args["b"].as_f64().ok_or("Missing parameter 'b'")?;
            if b == 0.0 {
                Err("Division by zero".to_string())
            } else {
                Ok(ToolOutput::text(format!("{}", a / b)))
            }
        }
        "echo" => {
            let message = args["message"]
                .as_str()
                .ok_or("Missing parameter 'message'")?;
            Ok(ToolOutput::text(message.to_string()))
        }
        "get_time" => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Ok(ToolOutput::text(format!("Current timestamp: {now}")))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

/// Read a resource.
fn read_resource(uri: &str) -> Result<ResourceContents, String> {
    match uri {
        "server://info" => Ok(ResourceContents::text(
            uri,
            serde_json::to_string_pretty(&json!({
                "name": "http-mcp-server",
                "version": "1.0.0",
                "transport": "HTTP/SSE",
                "protocol_version": MCP_PROTOCOL_VERSION,
            }))
            .unwrap(),
        )),
        "server://status" => Ok(ResourceContents::text(
            uri,
            serde_json::to_string_pretty(&json!({
                "status": "running",
                "uptime_seconds": 0,
                "connections": 1,
            }))
            .unwrap(),
        )),
        _ => Err(format!("Unknown resource: {uri}")),
    }
}

/// Get a prompt.
fn get_prompt(
    name: &str,
    args: Option<&serde_json::Map<String, Value>>,
) -> Result<GetPromptResult, String> {
    match name {
        "calculator" => {
            let operation = args
                .and_then(|a| a.get("operation"))
                .and_then(|v| v.as_str())
                .unwrap_or("general calculation");

            Ok(GetPromptResult::user(format!(
                "You are a helpful calculator assistant. Help the user with: {operation}. \
                 You have access to add, subtract, multiply, and divide tools."
            )))
        }
        "greeting" => {
            let name = args
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_str())
                .ok_or("Missing required argument 'name'")?;

            Ok(GetPromptResult::user(format!(
                "Generate a warm, friendly greeting for {name}."
            )))
        }
        _ => Err(format!("Unknown prompt: {name}")),
    }
}

/// Handle incoming JSON-RPC request.
async fn handle_request(state: &AppState, session_id: &str, request: &Request) -> JsonRpcResponse {
    let method: &str = &request.method;
    let params = request.params.clone().unwrap_or(Value::Null);

    match method {
        "initialize" => {
            let client_caps = params
                .get("capabilities")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            state.mark_initialized(session_id, client_caps).await;

            JsonRpcResponse::success(
                request.id.clone(),
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "serverInfo": {
                        "name": state.server_info.name,
                        "version": state.server_info.version,
                    },
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "resources": { "subscribe": false, "listChanged": false },
                        "prompts": { "listChanged": false },
                    },
                }),
            )
        }

        "tools/list" => {
            let tools: Vec<Value> = get_tools()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
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
                    json!({
                        "content": [{ "type": "text", "text": e }],
                        "isError": true,
                    }),
                ),
            }
        }

        "resources/list" => {
            let resources: Vec<Value> = get_resources()
                .into_iter()
                .map(|r| {
                    json!({
                        "uri": r.uri,
                        "name": r.name,
                        "description": r.description,
                        "mimeType": r.mime_type,
                    })
                })
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

        "prompts/list" => {
            let prompts: Vec<Value> = get_prompts()
                .into_iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "description": p.description,
                        "arguments": p.arguments,
                    })
                })
                .collect();

            JsonRpcResponse::success(request.id.clone(), json!({ "prompts": prompts }))
        }

        "prompts/get" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").and_then(|v| v.as_object());

            match get_prompt(name, args) {
                Ok(result) => JsonRpcResponse::success(
                    request.id.clone(),
                    serde_json::to_value(result).unwrap(),
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
            JsonRpcError {
                code: -32601,
                message: format!("Method not found: {method}"),
                data: None,
            },
        ),
    }
}

/// Handle POST requests with JSON-RPC messages.
async fn handle_mcp_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    // Check protocol version
    let version = headers
        .get(MCP_PROTOCOL_VERSION_HEADER)
        .and_then(|v| v.to_str().ok());

    if version != Some(MCP_PROTOCOL_VERSION) {
        return (
            StatusCode::BAD_REQUEST,
            format!("Missing or invalid {MCP_PROTOCOL_VERSION_HEADER} header. Expected: {MCP_PROTOCOL_VERSION}"),
        )
            .into_response();
    }

    // Get or create session
    let existing_session_id = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());

    let (session_id, is_new) = state.get_or_create_session(existing_session_id).await;

    if is_new {
        info!(session_id = %session_id, "New session created");
    }

    // Parse message
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message");
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response();
        }
    };

    // Process message
    let response_body = match msg {
        Message::Request(request) => {
            let response = handle_request(&state, &session_id, &request).await;
            serde_json::to_string(&Message::Response(response)).unwrap()
        }
        Message::Notification(notif) => {
            // Handle notifications (no response)
            if notif.method == "initialized" {
                info!(session_id = %session_id, "Client initialized");
            }
            // Return 202 Accepted for notifications
            return (
                StatusCode::ACCEPTED,
                [(MCP_SESSION_ID_HEADER, session_id.as_str())],
            )
                .into_response();
        }
        Message::Response(_) => {
            return (StatusCode::BAD_REQUEST, "Unexpected response message").into_response();
        }
    };

    // Return response with headers
    (
        StatusCode::OK,
        [
            (MCP_SESSION_ID_HEADER, session_id.as_str()),
            (MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION),
            ("Content-Type", "application/json"),
        ],
        response_body,
    )
        .into_response()
}

/// Handle GET requests with SSE streaming.
async fn handle_mcp_sse(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // Check Accept header
    let accept = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !accept.contains("text/event-stream") {
        return (StatusCode::NOT_ACCEPTABLE, "Must accept text/event-stream").into_response();
    }

    // Get session ID
    let session_id = match headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(id) => id.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing session ID").into_response();
        }
    };

    // Get SSE receiver for this session
    let rx = match state.get_sse_receiver(&session_id).await {
        Some(rx) => rx,
        None => {
            return (StatusCode::NOT_FOUND, "Session not found").into_response();
        }
    };

    info!(session_id = %session_id, "SSE stream opened");

    // Create SSE stream
    let stream = async_stream::stream! {
        let mut rx = rx;

        // Send initial connected event
        yield Ok::<_, Infallible>(Event::default().event("connected").data(session_id.clone()));

        // Forward events from the session
        loop {
            match rx.recv().await {
                Ok(data) => {
                    yield Ok(Event::default().event("message").data(data));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(session_id = %session_id, lagged = n, "SSE stream lagged");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!(session_id = %session_id, "SSE stream closed");
                    break;
                }
            }
        }
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

/// Handle DELETE requests to close sessions.
async fn handle_mcp_delete(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session_id = match headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
    {
        Some(id) => id,
        None => {
            return (StatusCode::BAD_REQUEST, "Missing session ID").into_response();
        }
    };

    state.remove_session(session_id).await;
    info!(session_id = %session_id, "Session closed");

    (StatusCode::OK, "Session closed").into_response()
}

/// Health check endpoint.
async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("http_server_example=info".parse().unwrap())
                .add_directive("tower_http=debug".parse().unwrap()),
        )
        .init();

    // Create application state
    let state = AppState::new();

    // Build router
    let app = Router::new()
        .route("/mcp", post(handle_mcp_post))
        .route("/mcp", get(handle_mcp_sse))
        .route("/mcp", delete(handle_mcp_delete))
        .route("/health", get(health_check))
        .with_state(state);

    // Start server
    // Use MCP_BIND_ADDR for containerized deployments (default: 127.0.0.1:3000)
    let addr = std::env::var("MCP_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    info!(addr = %addr, "Starting HTTP MCP server");
    println!("\nMCP HTTP Server running at http://{addr}/mcp");
    println!("\nTest with:");
    println!("  curl -X POST http://{addr}/mcp \\");
    println!("    -H \"Content-Type: application/json\" \\");
    println!("    -H \"MCP-Protocol-Version: {MCP_PROTOCOL_VERSION}\" \\");
    println!(
        "    -d '{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"{MCP_PROTOCOL_VERSION}\",\"clientInfo\":{{\"name\":\"curl\",\"version\":\"1.0\"}}}}}}'"
    );
    println!();

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
