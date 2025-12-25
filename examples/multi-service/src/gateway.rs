//! Gateway Service - Aggregates tools and resources from backend services.
//!
//! This demonstrates the API Gateway pattern for MCP, where a single gateway
//! presents a unified interface to clients while proxying requests to
//! specialized backend services.
//!
//! # Running
//!
//! First start the backend services:
//! ```bash
//! cargo run -p multi-service-example --bin tools-service &
//! cargo run -p multi-service-example --bin resources-service &
//! cargo run -p multi-service-example --bin gateway
//! ```
//!
//! # Architecture
//!
//! ```text
//! ┌──────────┐     ┌─────────┐
//! │  Client  │────▶│ Gateway │
//! └──────────┘     └────┬────┘
//!                       │
//!          ┌────────────┼────────────┐
//!          ▼            ▼            ▼
//!    ┌─────────┐  ┌──────────┐  ┌─────────┐
//!    │ Tools   │  │Resources │  │ Prompts │
//!    │ Service │  │ Service  │  │ Service │
//!    └─────────┘  └──────────┘  └─────────┘
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
    types::{GetPromptResult, Prompt},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tracing::{info, warn};

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Backend service client.
struct ServiceClient {
    endpoint: String,
    client: reqwest::Client,
}

impl ServiceClient {
    fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Forward a request to the backend service.
    async fn forward(&self, request: &Request) -> Option<JsonRpcResponse> {
        let msg = Message::Request(request.clone());
        let body = serde_json::to_string(&msg).ok()?;

        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
            .body(body)
            .send()
            .await
            .ok()?;

        let text = response.text().await.ok()?;
        let msg: Message = serde_json::from_str(&text).ok()?;

        match msg {
            Message::Response(resp) => Some(resp),
            _ => None,
        }
    }
}

/// Gateway application state.
struct AppState {
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
    tools_client: ServiceClient,
    resources_client: ServiceClient,
}

impl AppState {
    fn new() -> Self {
        Self {
            server_info: ServerInfo::new("mcp-gateway", "1.0.0"),
            capabilities: ServerCapabilities::new()
                .with_tools()
                .with_resources()
                .with_prompts(),
            tools_client: ServiceClient::new(&format!(
                "http://127.0.0.1:{}/mcp",
                common::ports::TOOLS
            )),
            resources_client: ServiceClient::new(&format!(
                "http://127.0.0.1:{}/mcp",
                common::ports::RESOURCES
            )),
        }
    }
}

/// Gateway prompts (provided directly by gateway).
fn get_prompts() -> Vec<Prompt> {
    vec![
        Prompt::new("system-overview").description("Get an overview of the multi-service system"),
        Prompt::new("debug-services").description("Debug information about connected services"),
    ]
}

/// Get a gateway prompt.
fn get_prompt(name: &str) -> Result<GetPromptResult, String> {
    match name {
        "system-overview" => Ok(GetPromptResult::user(
            "You are connected to a multi-service MCP gateway. \
             The system consists of: \
             1. Tools Service: Provides calculation and utility tools \
             2. Resources Service: Provides configuration and documentation \
             Use the available tools and resources to help the user."
                .to_string(),
        )),
        "debug-services" => Ok(GetPromptResult::user(format!(
            "Debug information:\n\
             - Gateway: Running on port {}\n\
             - Tools Service: http://127.0.0.1:{}/mcp\n\
             - Resources Service: http://127.0.0.1:{}/mcp",
            common::ports::GATEWAY,
            common::ports::TOOLS,
            common::ports::RESOURCES
        ))),
        _ => Err(format!("Unknown prompt: {name}")),
    }
}

/// Handle JSON-RPC request.
async fn handle_request(state: &AppState, request: &Request) -> JsonRpcResponse {
    let method = request.method.as_ref();
    let params = request.params.clone().unwrap_or(Value::Null);

    match method {
        // Gateway handles initialization
        "initialize" => JsonRpcResponse::success(
            request.id.clone(),
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "serverInfo": state.server_info,
                "capabilities": state.capabilities,
            }),
        ),

        // Aggregate tools from tools service
        "tools/list" => match state.tools_client.forward(request).await {
            Some(resp) => resp,
            None => {
                warn!("Tools service unavailable");
                JsonRpcResponse::success(request.id.clone(), json!({ "tools": [] }))
            }
        },

        // Forward tool calls to tools service
        "tools/call" => match state.tools_client.forward(request).await {
            Some(resp) => resp,
            None => JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError {
                    code: -32603,
                    message: "Tools service unavailable".to_string(),
                    data: None,
                },
            ),
        },

        // Aggregate resources from resources service
        "resources/list" => match state.resources_client.forward(request).await {
            Some(resp) => resp,
            None => {
                warn!("Resources service unavailable");
                JsonRpcResponse::success(request.id.clone(), json!({ "resources": [] }))
            }
        },

        // Forward resource reads to resources service
        "resources/read" => match state.resources_client.forward(request).await {
            Some(resp) => resp,
            None => JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError {
                    code: -32603,
                    message: "Resources service unavailable".to_string(),
                    data: None,
                },
            ),
        },

        // Gateway provides prompts directly
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
            match get_prompt(name) {
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

/// Health check - also checks backend services.
async fn health_check(State(state): State<Arc<AppState>>) -> Response {
    // Create ping requests for backend services
    let ping = Request {
        jsonrpc: "2.0".into(),
        id: mcpkit_core::protocol::RequestId::Number(0),
        method: "ping".into(),
        params: None,
    };

    let tools_ok = state.tools_client.forward(&ping).await.is_some();
    let resources_ok = state.resources_client.forward(&ping).await.is_some();

    let status = json!({
        "gateway": "ok",
        "tools_service": if tools_ok { "ok" } else { "unavailable" },
        "resources_service": if resources_ok { "ok" } else { "unavailable" },
    });

    if tools_ok && resources_ok {
        (StatusCode::OK, serde_json::to_string(&status).unwrap()).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            serde_json::to_string(&status).unwrap(),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() {
    common::init_tracing("gateway");

    let state = Arc::new(AppState::new());
    let addr = format!("0.0.0.0:{}", common::ports::GATEWAY);

    let app = Router::new()
        .route("/mcp", post(handle_mcp_post))
        .route("/health", get(health_check))
        .with_state(state);

    info!(addr = %addr, "Starting MCP Gateway");
    println!("MCP Gateway running at http://{addr}/mcp");
    println!();
    println!("Backend services:");
    println!(
        "  - Tools Service:     http://127.0.0.1:{}/mcp",
        common::ports::TOOLS
    );
    println!(
        "  - Resources Service: http://127.0.0.1:{}/mcp",
        common::ports::RESOURCES
    );
    println!();
    println!("Test with:");
    println!("  curl -X POST http://{addr}/mcp \\");
    println!("    -H \"Content-Type: application/json\" \\");
    println!("    -H \"MCP-Protocol-Version: {MCP_PROTOCOL_VERSION}\" \\");
    println!(
        "    -d '{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"{MCP_PROTOCOL_VERSION}\",\"clientInfo\":{{\"name\":\"curl\",\"version\":\"1.0\"}}}}}}'"
    );

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
