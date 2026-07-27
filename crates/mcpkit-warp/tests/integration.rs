//! Integration tests for mcpkit-warp.
//!
//! These tests verify the complete request/response flow through the Warp MCP integration.

use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
use mcpkit_server::ServerHandler;
use mcpkit_server::context::Context;
use mcpkit_server::handler::{PromptHandler, ResourceHandler, ToolHandler};
use mcpkit_warp::McpRouter;

/// Test MCP server handler.
struct TestHandler;

impl ServerHandler for TestHandler {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("test-warp-server", "1.0.0")
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

/// POST `initialize` against a filter and yield the assigned session id
/// (required since #153 PR 0b: only `initialize` may omit `mcp-session-id`).
/// A macro rather than a fn: `warp::test`'s filter bounds are unnameable.
macro_rules! init_session {
    ($filter:expr) => {{
        let response = warp::test::request()
            .method("POST")
            .path("/mcp")
            .header("content-type", "application/json")
            .header("mcp-protocol-version", "2025-11-25")
            .body(r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":0}"#)
            .reply($filter)
            .await;
        response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .expect("initialize must assign a session id")
    }};
}

#[tokio::test]
async fn test_ping_request() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_initialize_request() {
    let filter = McpRouter::new(TestHandler).into_filter();

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .body(r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);

    let body = String::from_utf8_lossy(response.body());
    assert!(body.contains("protocolVersion"));
    assert!(body.contains("serverInfo"));
    assert!(body.contains("capabilities"));
}

#[tokio::test]
async fn test_unsupported_protocol_version() {
    let filter = McpRouter::new(TestHandler).into_filter();

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "unsupported-version")
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_list_tools() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);

    let body = String::from_utf8_lossy(response.body());
    assert!(body.contains("echo"));
    assert!(body.contains("add"));
}

#[tokio::test]
async fn test_call_tool() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(
            r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"echo","arguments":{"message":"hello"}},"id":1}"#,
        )
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);

    let body = String::from_utf8_lossy(response.body());
    assert!(body.contains("Echo: hello"));
}

#[tokio::test]
async fn test_notification() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 202);
}

#[tokio::test]
async fn test_invalid_json() {
    let filter = McpRouter::new(TestHandler).into_filter();

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .body("not valid json")
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_method_not_found() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(r#"{"jsonrpc":"2.0","method":"unknown/method","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);

    let body = String::from_utf8_lossy(response.body());
    assert!(body.contains("error"));
    assert!(body.contains("not found"));
}

#[tokio::test]
async fn test_cors_headers() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .header("origin", "http://localhost:3000")
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_some()
    );
}

#[tokio::test]
async fn test_cors_preflight() {
    let filter = McpRouter::new(TestHandler).into_filter();

    let response = warp::test::request()
        .method("OPTIONS")
        .path("/mcp")
        .header("origin", "http://localhost:3000")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "content-type,mcp-protocol-version",
        )
        .reply(&filter)
        .await;

    // CORS preflight should be handled
    assert!(response.status() == 200 || response.status() == 204);
}

#[tokio::test]
async fn test_without_cors() {
    let filter = McpRouter::new(TestHandler).into_filter_without_cors();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 200);
    // Without CORS, there should be no CORS headers
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
}

/// Spec (Streamable HTTP): a `POSTed` JSON-RPC *response* is accepted with
/// 202, not rejected (#153 PR 0a).
#[tokio::test]
async fn response_post_is_accepted_with_202() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .header("mcp-session-id", sid.as_str())
        .body(r#"{"jsonrpc":"2.0","id":42,"result":{"roots":[]}}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 202);
}

/// #153 PR 0b: a non-initialize message without `mcp-session-id` is 400.
#[tokio::test]
async fn missing_session_id_on_non_initialize_is_400() {
    let filter = McpRouter::new(TestHandler).into_filter();

    let response = warp::test::request()
        .method("POST")
        .path("/mcp")
        .header("content-type", "application/json")
        .header("mcp-protocol-version", "2025-11-25")
        .body(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .reply(&filter)
        .await;

    assert_eq!(response.status(), 400);
}

/// An id the session's task store does not own is *invalid params* (-32602),
/// not *method not found* (-32601). The adapters have no custom task handler,
/// so a store that declines an id has declined it for good.
#[tokio::test]
async fn unknown_task_id_is_invalid_params_not_method_not_found() {
    let filter = McpRouter::new(TestHandler).into_filter();
    let sid = init_session!(&filter);

    for method in ["tasks/get", "tasks/result", "tasks/cancel"] {
        let response = warp::test::request()
            .method("POST")
            .path("/mcp")
            .header("content-type", "application/json")
            .header("mcp-protocol-version", "2025-11-25")
            .header("mcp-session-id", sid.as_str())
            .body(format!(
                r#"{{"jsonrpc":"2.0","method":"{method}","params":{{"taskId":"no-such-task-id"}},"id":1}}"#
            ))
            .reply(&filter)
            .await;

        let json: serde_json::Value = serde_json::from_slice(response.body()).expect("json body");
        assert_eq!(
            json["error"]["code"], -32602,
            "{method} with an unknown id must be -32602: {json}"
        );
    }
}

use mcpkit_warp::SessionStore;

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
