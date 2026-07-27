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
        args: serde_json::Map<String, serde_json::Value>,
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
                meta: None,
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
        .mount("/", rocket::routes![mcp_post, mcp_get, mcp_delete, mcp_sse]);
    Client::tracked(rocket).expect("valid rocket instance")
}

/// POST `initialize` and return the assigned session id (required since
/// #153 PR 0b: only `initialize` may omit `mcp-session-id`).
fn init_session(client: &Client) -> String {
    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":0}"#)
        .dispatch();
    response
        .headers()
        .get_one("mcp-session-id")
        .map(String::from)
        .expect("initialize must assign a session id")
}

#[test]
fn test_ping_request() {
    let client = create_test_client();
    let sid = init_session(&client);

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
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

    // First request: initialize assigns the session (#153 PR 0b: only
    // initialize may omit mcp-session-id).
    let session_id = init_session(&client);

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
    let sid = init_session(&client);

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
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
    let sid = init_session(&client);

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
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
    let sid = init_session(&client);

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
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
    let sid = init_session(&client);

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
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
    let sid = init_session(&client);

    // Make a normal request and check CORS headers are present
    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
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

/// Spec (Streamable HTTP): a POSTed JSON-RPC *response* is accepted with
/// 202, not rejected (#153 PR 0a).
#[test]
fn response_post_is_accepted_with_202() {
    let client = create_test_client();
    let sid = init_session(&client);

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .header(Header::new("mcp-session-id", sid))
        .body(r#"{"jsonrpc":"2.0","id":42,"result":{"roots":[]}}"#)
        .dispatch();

    assert_eq!(response.status(), Status::Accepted);
}

/// #153 PR 0b: a non-initialize message without `mcp-session-id` is 400.
#[test]
fn missing_session_id_on_non_initialize_is_400() {
    let client = create_test_client();

    let response = client
        .post("/mcp")
        .header(ContentType::JSON)
        .header(Header::new("mcp-protocol-version", "2025-11-25"))
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .dispatch();

    assert_eq!(response.status(), Status::BadRequest);
}

/// An id the session's task store does not own is *invalid params* (-32602),
/// not *method not found* (-32601). The adapters have no custom task handler,
/// so a store that declines an id has declined it for good.
#[test]
fn unknown_task_id_is_invalid_params_not_method_not_found() {
    let client = create_test_client();
    let sid = init_session(&client);

    for method in ["tasks/get", "tasks/result", "tasks/cancel"] {
        let response = client
            .post("/mcp")
            .header(ContentType::JSON)
            .header(Header::new("mcp-protocol-version", "2025-11-25"))
            .header(Header::new("mcp-session-id", sid.clone()))
            .body(format!(
                r#"{{"jsonrpc":"2.0","method":"{method}","params":{{"taskId":"no-such-task-id"}},"id":1}}"#
            ))
            .dispatch();

        let body = response.into_string().expect("body");
        let json: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(
            json["error"]["code"], -32602,
            "{method} with an unknown id must be -32602: {json}"
        );
    }
}

use mcpkit_rocket::SessionStore;

/// Every session this adapter creates must build its task store *wired to its
/// own stream registry*, or `notifications/tasks/status` is implemented and
/// never emitted on this transport.
#[tokio::test]
async fn session_task_store_is_wired_to_the_session_stream() {
    let store = SessionStore::new();
    let id = store.create();
    let tasks = store.tasks(&id).expect("session has a task store");
    let streams = store.streams(&id).expect("session has a stream registry");

    let (mut stream, _prime) = streams.open("message", "{}".to_string());
    let task = tasks.create(None);
    task.complete(serde_json::json!({"ok": true}))
        .expect("complete");

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), stream.recv())
        .await
        .expect("no notification reached the session stream")
        .expect("stream closed");
    let json: serde_json::Value = serde_json::from_str(&event.data).expect("json");
    assert_eq!(json["method"], "notifications/tasks/status");
    assert_eq!(json["params"]["taskId"], task.id().as_str());
    assert_eq!(json["params"]["status"], "completed");
}
