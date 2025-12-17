//! MCP WebSocket Server Example
//!
//! This example demonstrates a full MCP server using WebSocket transport.
//! WebSocket provides full-duplex communication, making it ideal for
//! long-running sessions with bidirectional message flow.
//!
//! # Running
//!
//! ```bash
//! cargo run -p websocket-server-example
//! ```
//!
//! The server will listen on `ws://127.0.0.1:3001`.
//!
//! # Testing with websocat
//!
//! ```bash
//! websocat ws://127.0.0.1:3001
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"websocat","version":"1.0"}}}
//! ```

use futures::{SinkExt, StreamExt};
use mcpkit_core::{
    capability::{ServerCapabilities, ServerInfo},
    error::JsonRpcError,
    protocol::{Message, Request, Response as JsonRpcResponse},
    types::{
        CallToolResult, GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput,
    },
};
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
};
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};
use tracing::{error, info, warn};

/// MCP Protocol version.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Connection state.
#[derive(Debug, Default)]
struct ConnectionState {
    /// Whether initialized.
    initialized: bool,
    /// Request counter.
    request_count: u64,
}

/// Server state shared between connections.
struct ServerState {
    /// Server info.
    info: ServerInfo,
    /// Server capabilities.
    capabilities: ServerCapabilities,
    /// Active connections.
    connections: RwLock<HashMap<SocketAddr, ConnectionState>>,
}

impl ServerState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            info: ServerInfo::new("websocket-mcp-server", "1.0.0"),
            capabilities: ServerCapabilities::new()
                .with_tools()
                .with_resources()
                .with_prompts(),
            connections: RwLock::new(HashMap::new()),
        })
    }
}

/// Get available tools.
fn get_tools() -> Vec<Tool> {
    vec![
        Tool::new("calculate")
            .description("Perform a mathematical calculation")
            .with_string_param("expression", "Mathematical expression to evaluate", true),
        Tool::new("reverse")
            .description("Reverse a string")
            .with_string_param("text", "Text to reverse", true),
        Tool::new("uppercase")
            .description("Convert text to uppercase")
            .with_string_param("text", "Text to convert", true),
        Tool::new("lowercase")
            .description("Convert text to lowercase")
            .with_string_param("text", "Text to convert", true),
        Tool::new("word_count")
            .description("Count words in text")
            .with_string_param("text", "Text to count words in", true),
        Tool::new("random_number")
            .description("Generate a random number")
            .with_number_param("min", "Minimum value (default: 0)", false)
            .with_number_param("max", "Maximum value (default: 100)", false),
    ]
}

/// Get available resources.
fn get_resources() -> Vec<Resource> {
    vec![
        Resource::new("ws://server/info", "Server Information")
            .mime_type("application/json")
            .description("Information about this WebSocket MCP server"),
        Resource::new("ws://server/capabilities", "Server Capabilities")
            .mime_type("application/json")
            .description("Detailed server capability information"),
        Resource::new("ws://server/stats", "Connection Statistics")
            .mime_type("application/json")
            .description("Current connection statistics"),
    ]
}

/// Get available prompts.
fn get_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new("text_processor")
            .description("Process text with various transformations")
            .required_arg("text", "The text to process")
            .optional_arg("operation", "Operation to perform"),
        Prompt::new("math_helper")
            .description("Help with mathematical calculations")
            .optional_arg("problem", "The math problem to solve"),
    ]
}

/// Execute a tool.
fn call_tool(name: &str, args: &Value) -> Result<ToolOutput, String> {
    match name {
        "calculate" => {
            let expr = args["expression"]
                .as_str()
                .ok_or("Missing 'expression' parameter")?;

            // Simple expression evaluator (very basic)
            let result = eval_simple_expr(expr)?;
            Ok(ToolOutput::text(format!("{expr} = {result}")))
        }

        "reverse" => {
            let text = args["text"].as_str().ok_or("Missing 'text' parameter")?;
            let reversed: String = text.chars().rev().collect();
            Ok(ToolOutput::text(reversed))
        }

        "uppercase" => {
            let text = args["text"].as_str().ok_or("Missing 'text' parameter")?;
            Ok(ToolOutput::text(text.to_uppercase()))
        }

        "lowercase" => {
            let text = args["text"].as_str().ok_or("Missing 'text' parameter")?;
            Ok(ToolOutput::text(text.to_lowercase()))
        }

        "word_count" => {
            let text = args["text"].as_str().ok_or("Missing 'text' parameter")?;
            let count = text.split_whitespace().count();
            Ok(ToolOutput::text(format!("{count} words")))
        }

        "random_number" => {
            let min = args["min"].as_f64().unwrap_or(0.0) as i64;
            let max = args["max"].as_f64().unwrap_or(100.0) as i64;

            if min >= max {
                return Err("min must be less than max".to_string());
            }

            // Simple pseudo-random using time
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as i64;
            let range = max - min;
            let random = min + (seed.abs() % range);

            Ok(ToolOutput::text(format!("{random}")))
        }

        _ => Err(format!("Unknown tool: {name}")),
    }
}

/// Simple expression evaluator (addition and subtraction only for safety).
fn eval_simple_expr(expr: &str) -> Result<f64, String> {
    let expr = expr.replace(' ', "");
    let mut result = 0.0;
    let mut current_num = String::new();
    let mut current_op = '+';

    for ch in expr.chars().chain(std::iter::once('+')) {
        if ch.is_ascii_digit() || ch == '.' {
            current_num.push(ch);
        } else if ch == '+' || ch == '-' || ch == '*' || ch == '/' {
            if !current_num.is_empty() {
                let num: f64 = current_num
                    .parse()
                    .map_err(|_| format!("Invalid number: {current_num}"))?;
                current_num.clear();

                match current_op {
                    '+' => result += num,
                    '-' => result -= num,
                    '*' => result *= num,
                    '/' => {
                        if num == 0.0 {
                            return Err("Division by zero".to_string());
                        }
                        result /= num;
                    }
                    _ => {}
                }
            }
            current_op = ch;
        } else {
            return Err(format!("Invalid character in expression: {ch}"));
        }
    }

    Ok(result)
}

/// Read a resource.
fn read_resource(uri: &str, state: &ServerState) -> Result<ResourceContents, String> {
    match uri {
        "ws://server/info" => Ok(ResourceContents::text(
            uri,
            serde_json::to_string_pretty(&json!({
                "name": state.info.name,
                "version": state.info.version,
                "transport": "WebSocket",
                "protocol_version": MCP_PROTOCOL_VERSION,
            }))
            .unwrap(),
        )),

        "ws://server/capabilities" => Ok(ResourceContents::text(
            uri,
            serde_json::to_string_pretty(&json!({
                "tools": state.capabilities.has_tools(),
                "resources": state.capabilities.has_resources(),
                "prompts": state.capabilities.has_prompts(),
                "experimental": {},
            }))
            .unwrap(),
        )),

        "ws://server/stats" => {
            // Would need async access to connections in real implementation
            Ok(ResourceContents::text(
                uri,
                serde_json::to_string_pretty(&json!({
                    "active_connections": 1,
                    "transport": "WebSocket",
                }))
                .unwrap(),
            ))
        }

        _ => Err(format!("Unknown resource: {uri}")),
    }
}

/// Get a prompt.
fn get_prompt(
    name: &str,
    args: Option<&serde_json::Map<String, Value>>,
) -> Result<GetPromptResult, String> {
    match name {
        "text_processor" => {
            let text = args
                .and_then(|a| a.get("text"))
                .and_then(|v| v.as_str())
                .ok_or("Missing required argument 'text'")?;

            let operation = args
                .and_then(|a| a.get("operation"))
                .and_then(|v| v.as_str())
                .unwrap_or("analyze");

            Ok(GetPromptResult::user(format!(
                "You are a text processing assistant. {operation} the following text:\n\n{text}\n\n\
                 You have access to tools: reverse, uppercase, lowercase, word_count."
            )))
        }

        "math_helper" => {
            let problem = args
                .and_then(|a| a.get("problem"))
                .and_then(|v| v.as_str())
                .unwrap_or("general math");

            Ok(GetPromptResult::user(format!(
                "You are a math tutor. Help solve: {problem}\n\n\
                 You have access to the 'calculate' tool for evaluating expressions."
            )))
        }

        _ => Err(format!("Unknown prompt: {name}")),
    }
}

/// Handle a JSON-RPC request.
fn handle_request(state: &ServerState, request: &Request) -> JsonRpcResponse {
    let method: &str = &request.method;
    let params = request.params.clone().unwrap_or(Value::Null);

    match method {
        "initialize" => JsonRpcResponse::success(
            request.id.clone(),
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": {
                    "name": state.info.name,
                    "version": state.info.version,
                },
                "capabilities": {
                    "tools": { "listChanged": false },
                    "resources": { "subscribe": false, "listChanged": false },
                    "prompts": { "listChanged": false },
                },
            }),
        ),

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

            match read_resource(uri, state) {
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

/// Handle a WebSocket connection.
async fn handle_connection(stream: TcpStream, addr: SocketAddr, state: Arc<ServerState>) {
    info!(addr = %addr, "New WebSocket connection");

    // Accept WebSocket handshake
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!(addr = %addr, error = %e, "WebSocket handshake failed");
            return;
        }
    };

    let (mut tx, mut rx) = ws_stream.split();

    // Track connection
    state
        .connections
        .write()
        .await
        .insert(addr, ConnectionState::default());

    // Process messages
    while let Some(msg) = rx.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                error!(addr = %addr, error = %e, "WebSocket receive error");
                break;
            }
        };

        match msg {
            WsMessage::Text(text) => {
                // Parse JSON-RPC message
                let mcp_msg: Message = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        warn!(addr = %addr, error = %e, "Invalid JSON-RPC message");

                        // Send parse error
                        let error_response = json!({
                            "jsonrpc": "2.0",
                            "id": null,
                            "error": {
                                "code": -32700,
                                "message": format!("Parse error: {e}"),
                            },
                        });

                        if let Err(e) = tx.send(WsMessage::Text(error_response.to_string())).await {
                            error!(addr = %addr, error = %e, "Failed to send error response");
                            break;
                        }
                        continue;
                    }
                };

                // Handle message
                let response = match mcp_msg {
                    Message::Request(ref request) => {
                        // Update request count
                        if let Some(conn_state) = state.connections.write().await.get_mut(&addr) {
                            conn_state.request_count += 1;
                            if request.method == "initialize" {
                                conn_state.initialized = true;
                            }
                        }

                        let response = handle_request(&state, request);
                        Some(Message::Response(response))
                    }
                    Message::Notification(notif) => {
                        // Handle notification (no response)
                        if notif.method == "initialized" {
                            info!(addr = %addr, "Client initialized");
                        }
                        None
                    }
                    Message::Response(_) => {
                        warn!(addr = %addr, "Unexpected response message from client");
                        None
                    }
                };

                // Send response if any
                if let Some(resp) = response {
                    let json = serde_json::to_string(&resp).unwrap();
                    if let Err(e) = tx.send(WsMessage::Text(json)).await {
                        error!(addr = %addr, error = %e, "Failed to send response");
                        break;
                    }
                }
            }

            WsMessage::Ping(data) => {
                if let Err(e) = tx.send(WsMessage::Pong(data)).await {
                    error!(addr = %addr, error = %e, "Failed to send pong");
                    break;
                }
            }

            WsMessage::Close(_) => {
                info!(addr = %addr, "Client requested close");
                break;
            }

            _ => {}
        }
    }

    // Clean up connection
    state.connections.write().await.remove(&addr);
    info!(addr = %addr, "WebSocket connection closed");
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("websocket_server_example=info".parse().unwrap()),
        )
        .init();

    // Create server state
    let state = ServerState::new();

    // Bind TCP listener
    let addr = "127.0.0.1:3001";
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");

    info!(addr = %addr, "Starting WebSocket MCP server");
    println!("\nMCP WebSocket Server running at ws://{addr}");
    println!("\nTest with websocat:");
    println!("  websocat ws://{addr}");
    println!();
    println!("Then send:");
    println!(
        r#"  {{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"{MCP_PROTOCOL_VERSION}","clientInfo":{{"name":"websocat","version":"1.0"}}}}}}"#
    );
    println!();

    // Accept connections
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    handle_connection(stream, addr, state).await;
                });
            }
            Err(e) => {
                error!(error = %e, "Failed to accept connection");
            }
        }
    }
}
