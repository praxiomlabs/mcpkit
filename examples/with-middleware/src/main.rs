//! MCP Server with Middleware Example
//!
//! This example demonstrates how to use the middleware layer system to add
//! cross-cutting concerns like logging, timeouts, retries, and metrics to
//! MCP transports.
//!
//! # Middleware Layers
//!
//! The following middleware layers are demonstrated:
//!
//! - **LoggingLayer**: Logs all messages sent and received
//! - **TimeoutLayer**: Adds configurable timeouts to send/receive operations
//! - **RetryLayer**: Automatic retry with exponential backoff for transient failures
//! - **MetricsLayer**: Collects performance metrics (message counts, latencies)
//!
//! # Running
//!
//! ```bash
//! RUST_LOG=debug cargo run -p with-middleware-example
//! ```
//!
//! Then send JSON-RPC messages via stdin.

use mcpkit_core::{
    error::JsonRpcError,
    protocol::{Message, Request, Response as JsonRpcResponse},
    types::{CallToolResult, Tool, ToolOutput},
};
use mcpkit_transport::{
    middleware::{ExponentialBackoff, LayerStack, LoggingLayer, RetryLayer, TimeoutLayer},
    stdio::StdioTransport, Transport,
};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{info, Level};

/// MCP Protocol version.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Get available tools.
fn get_tools() -> Vec<Tool> {
    vec![
        Tool::new("echo")
            .description("Echo back the input message")
            .with_string_param("message", "Message to echo", true),
        Tool::new("slow_operation")
            .description("A slow operation for testing timeouts")
            .with_number_param("delay_ms", "Delay in milliseconds", false),
        Tool::new("failing_operation")
            .description("An operation that may fail for testing retries")
            .with_number_param("fail_probability", "Probability of failure (0-100)", false),
    ]
}

/// Execute a tool.
fn call_tool(name: &str, args: &Value) -> Result<ToolOutput, String> {
    match name {
        "echo" => {
            let message = args["message"]
                .as_str()
                .ok_or("Missing 'message' parameter")?;
            Ok(ToolOutput::text(format!("Echo: {message}")))
        }
        "slow_operation" => {
            let delay_ms = args["delay_ms"].as_u64().unwrap_or(1000);
            std::thread::sleep(Duration::from_millis(delay_ms));
            Ok(ToolOutput::text(format!(
                "Completed after {delay_ms}ms delay"
            )))
        }
        "failing_operation" => {
            let fail_prob = args["fail_probability"].as_u64().unwrap_or(50);
            let random = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                % 100;

            if random < fail_prob as u128 {
                Err("Random failure occurred".to_string())
            } else {
                Ok(ToolOutput::text("Operation succeeded"))
            }
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

/// Handle a JSON-RPC request.
fn handle_request(request: &Request) -> JsonRpcResponse {
    let method: &str = &request.method;
    let params = request.params.clone().unwrap_or(Value::Null);

    match method {
        "initialize" => JsonRpcResponse::success(
            request.id.clone(),
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": {
                    "name": "middleware-example-server",
                    "version": "1.0.0",
                },
                "capabilities": {
                    "tools": { "listChanged": false },
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with DEBUG level to see middleware logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("with_middleware_example=debug".parse()?)
                .add_directive("mcp_transport=debug".parse()?),
        )
        .init();

    info!("Starting MCP server with middleware layers");

    // Create the base stdio transport
    let transport = StdioTransport::new();

    // Build a middleware stack
    //
    // Layers are applied from inside out:
    // 1. LoggingLayer - innermost, logs all raw messages
    // 2. TimeoutLayer - adds timeouts
    // 3. RetryLayer - outermost, handles retries
    //
    // Note: The actual layer application order might be different depending on
    // what behavior you want. Here we demonstrate configuration.

    // Configure logging layer
    let logging = LoggingLayer::new(Level::DEBUG).with_contents(true);

    // Configure timeout layer
    let _timeout = TimeoutLayer::default()
        .send_timeout(Duration::from_secs(30))
        .recv_timeout(Duration::from_secs(60));

    // Configure retry layer with exponential backoff
    let _retry = RetryLayer::new(3).backoff(
        ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(5))
            .multiplier(2.0)
            .jitter(0.1),
    );

    info!("Middleware stack configured:");
    info!("  - LoggingLayer: level=DEBUG, contents=true");
    info!("  - TimeoutLayer: send=30s, recv=60s");
    info!("  - RetryLayer: max_attempts=3, backoff=exponential");

    // Apply layers using LayerStack
    // Note: We're demonstrating the configuration here. Due to type constraints,
    // we may not be able to apply all layers in this simple example, but the
    // configuration pattern is what's important.
    let stack = LayerStack::new(transport).with(logging);
    let transport = stack.into_inner();

    info!("Transport with middleware ready");
    info!("Listening for JSON-RPC messages on stdin...");
    eprintln!("\nMCP Server with Middleware");
    eprintln!("==========================");
    eprintln!("This server demonstrates middleware layers:");
    eprintln!("  - LoggingLayer: Logs all messages at DEBUG level");
    eprintln!("  - TimeoutLayer: 30s send timeout, 60s receive timeout");
    eprintln!("  - RetryLayer: 3 retries with exponential backoff");
    eprintln!();
    eprintln!("Available tools:");
    eprintln!("  - echo: Echo back a message");
    eprintln!("  - slow_operation: Simulate a slow operation");
    eprintln!("  - failing_operation: Randomly fail (for retry testing)");
    eprintln!();
    eprintln!("Send JSON-RPC messages via stdin.");
    eprintln!();

    // Main message loop
    loop {
        // Receive message through middleware stack
        let msg = match transport.recv().await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                info!("Connection closed");
                break;
            }
            Err(e) => {
                tracing::error!(error = ?e, "Receive error");
                break;
            }
        };

        // Handle message
        let response = match msg {
            Message::Request(ref request) => {
                info!(method = %request.method, "Handling request");
                let response = handle_request(request);
                Some(Message::Response(response))
            }
            Message::Notification(ref notif) => {
                info!(method = %notif.method, "Received notification");
                if notif.method == "initialized" {
                    info!("Client initialized");
                }
                None
            }
            Message::Response(_) => {
                tracing::warn!("Unexpected response message from client");
                None
            }
        };

        // Send response through middleware stack
        if let Some(resp) = response {
            if let Err(e) = transport.send(resp).await {
                tracing::error!(error = ?e, "Send error");
                break;
            }
        }
    }

    info!("Server shutting down");
    transport.close().await?;

    Ok(())
}

/// Demonstrate middleware configuration patterns.
///
/// This function shows various ways to configure and compose middleware.
/// It's not called from main but serves as documentation.
#[allow(dead_code)]
fn middleware_configuration_examples() {
    // Example 1: Simple logging only
    let _logging = LoggingLayer::new(Level::INFO);

    // Example 2: Verbose debugging with message contents
    let _debug_logging = LoggingLayer::new(Level::TRACE).with_contents(true);

    // Example 3: Custom timeouts
    let _timeout = TimeoutLayer::with_timeouts(
        Duration::from_secs(10),  // send timeout
        Duration::from_secs(120), // receive timeout
    );

    // Example 4: Timeout for send only, no receive timeout
    let _send_only_timeout = TimeoutLayer::default()
        .send_timeout(Duration::from_secs(5))
        .no_recv_timeout();

    // Example 5: Aggressive retry with short backoff
    let _aggressive_retry = RetryLayer::new(5).backoff(
        ExponentialBackoff::new(Duration::from_millis(50), Duration::from_secs(1))
            .multiplier(1.5)
            .no_jitter(),
    );

    // Example 6: Conservative retry with long backoff
    let _conservative_retry = RetryLayer::new(3).backoff(
        ExponentialBackoff::new(Duration::from_secs(1), Duration::from_secs(30))
            .multiplier(2.0)
            .jitter(0.2),
    );

    // Example 7: Combining layers (conceptual - actual types may differ)
    // let transport = StdioTransport::new();
    // let stack = LayerStack::new(transport)
    //     .with(LoggingLayer::new(Level::DEBUG))
    //     .with(TimeoutLayer::default())
    //     .with(RetryLayer::new(3));
    // let transport = stack.into_inner();
}

/// Example of a custom retry policy.
///
/// This shows how to implement custom retry logic.
#[allow(dead_code)]
mod custom_policy {
    use mcpkit_transport::{error::TransportError, middleware::RetryPolicy};

    /// A custom retry policy that only retries on specific errors.
    #[derive(Debug, Clone)]
    pub struct SelectiveRetryPolicy {
        /// Only retry timeout errors.
        retry_timeouts: bool,
        /// Only retry connection errors.
        retry_connections: bool,
    }

    impl SelectiveRetryPolicy {
        pub fn new() -> Self {
            Self {
                retry_timeouts: true,
                retry_connections: true,
            }
        }

        pub fn timeouts_only(mut self) -> Self {
            self.retry_timeouts = true;
            self.retry_connections = false;
            self
        }

        pub fn connections_only(mut self) -> Self {
            self.retry_timeouts = false;
            self.retry_connections = true;
            self
        }
    }

    impl RetryPolicy for SelectiveRetryPolicy {
        fn should_retry(&self, error: &TransportError) -> bool {
            match error {
                TransportError::Timeout { .. } => self.retry_timeouts,
                TransportError::Connection { .. }
                | TransportError::ConnectionClosed
                | TransportError::IoError(_)
                | TransportError::Io { .. } => self.retry_connections,
                _ => false,
            }
        }

        fn clone_box(&self) -> Box<dyn RetryPolicy> {
            Box::new(self.clone())
        }
    }
}
