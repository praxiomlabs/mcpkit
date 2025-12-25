//! Integration tests for mcpkit-rocket.
//!
//! These tests verify the complete request/response flow through the Rocket MCP integration.

use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
use mcpkit_rocket::Cors;
use mcpkit_rocket::prelude::*;
use mcpkit_server::ServerHandler;
use mcpkit_server::context::Context;
use mcpkit_server::handler::{PromptHandler, ResourceHandler, ToolHandler};
use rocket::http::{ContentType, Header, Status};
use rocket::local::blocking::Client;

/// Test MCP server handler.
struct TestHandler;

impl ServerHandler for TestHandler {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("test-rocket-server", "1.0.0")
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new()
            .with_tools()
            .with_resources()
            .with_prompts()
    }
}

impl ToolHandler for TestHandler {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool::new("echo").description("Echo input back"),
            Tool::new("add").description("Add two numbers"),
        ])
    }

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "echo" => {
                let msg = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                Ok(ToolOutput::text(format!("Echo: {msg}")))
            }
            "add" => {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                Ok(ToolOutput::text(format!("{}", a + b)))
            }
            _ => Err(McpError::tool_error(name, "Tool not found")),
        }
    }
}

impl ResourceHandler for TestHandler {
    async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
        Ok(vec![Resource::new("file:///test.txt", "Test File")])
    }

    async fn read_resource(
        &self,
        uri: &str,
        _ctx: &Context<'_>,
    ) -> Result<Vec<ResourceContents>, McpError> {
        if uri == "file:///test.txt" {
            Ok(vec![ResourceContents::text(uri, "Hello from test file!")])
        } else {
            Err(McpError::resource_not_found(uri))
        }
    }
}

impl PromptHandler for TestHandler {
    async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
        Ok(vec![
            Prompt::new("greeting").description("A greeting prompt"),
        ])
    }

    async fn get_prompt(
        &self,
        name: &str,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        if name == "greeting" {
            Ok(GetPromptResult {
                description: Some("A friendly greeting".to_string()),
                messages: vec![],
            })
        } else {
            Err(McpError::method_not_found(format!("prompts/get:{name}")))
        }
    }
}

// Generate the MCP routes for TestHandler
mcpkit_rocket::create_mcp_routes!(TestHandler);

fn create_test_client() -> Client {
    let state = McpRouter::new(TestHandler).into_state();
    let rocket = rocket::build()
        .manage(state)
        .mount("/", rocket::routes![mcp_post, mcp_sse]);
    Client::tracked(rocket).expect("valid rocket instance")
}

#[test]
fn test_ping_request() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert!(response.headers().get_one("mcp-session-id").is_some());
}

#[test]
fn test_initialize_request() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = response.into_string().unwrap();
    assert!(body.contains("protocolVersion"));
    assert!(body.contains("serverInfo"));
    assert!(body.contains("capabilities"));
}

#[test]
fn test_unsupported_protocol_version() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "unsupported-version"))
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::BadRequest);
}

#[test]
fn test_session_persistence() {
    let client = create_test_client();

    // First request - get a session
    let response1 = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .dispatch();

    let session_id = response1
        .headers()
        .get_one("mcp-session-id")
        .unwrap()
        .to_string();

    // Second request - reuse session
    let response2 = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", session_id.clone()))
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":2}"#)
        .dispatch();

    assert_eq!(response2.status(), Status::Ok);
    assert_eq!(
        response2.headers().get_one("mcp-session-id"),
        Some(session_id.as_str())
    );
}

#[test]
fn test_list_tools() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = response.into_string().unwrap();
    assert!(body.contains("echo"));
    assert!(body.contains("add"));
}

#[test]
fn test_call_tool() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(
            r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"echo","arguments":{"message":"hello"}},"id":1}"#,
        )
        .dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = response.into_string().unwrap();
    assert!(body.contains("Echo: hello"));
}

#[test]
fn test_notification() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Accepted);
}

#[test]
fn test_invalid_json() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body("not valid json")
        .dispatch();

    assert_eq!(response.status(), Status::BadRequest);
}

#[test]
fn test_method_not_found() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"unknown/method","id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);

    let body = response.into_string().unwrap();
    assert!(body.contains("error"));
    assert!(body.contains("not found"));
}

#[test]
fn test_cors_headers() {
    let state = McpRouter::new(TestHandler).into_state();
    let rocket = rocket::build()
        .manage(state)
        .mount("/", rocket::routes![mcp_post, mcp_sse])
        .attach(Cors);
    let client = Client::tracked(rocket).expect("valid rocket instance");

    // Make a normal request and check CORS headers are present
    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("Origin", "http://localhost:3000"))
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Ok);
    assert!(
        response
            .headers()
            .get_one("Access-Control-Allow-Origin")
            .is_some()
    );
}
