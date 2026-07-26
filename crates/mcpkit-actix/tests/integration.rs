//! Adapter-level HTTP tests for the actix adapter.

use actix_web::{App, test};
use mcpkit_actix::McpRouter;
use mcpkit_core::capability::ServerInfo;
use mcpkit_core::error::McpError;
use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};

struct TestHandler;

impl ServerHandler for TestHandler {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("t", "1.0.0")
    }
}
impl ToolHandler for TestHandler {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![])
    }
    async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Map<String, serde_json::Value>,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        Err(McpError::method_not_found(name))
    }
}
impl ResourceHandler for TestHandler {
    async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
        Ok(vec![])
    }
    async fn read_resource(
        &self,
        _uri: &str,
        _ctx: &Context<'_>,
    ) -> Result<Vec<ResourceContents>, McpError> {
        Ok(vec![])
    }
}
impl PromptHandler for TestHandler {
    async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
        Ok(vec![])
    }
    async fn get_prompt(
        &self,
        name: &str,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::method_not_found(name))
    }
}

/// Spec (Streamable HTTP): a POSTed JSON-RPC *response* is accepted with
/// 202, not rejected (#153 PR 0a).
#[actix_rt::test]
async fn response_post_is_accepted_with_202() {
    let router = McpRouter::new(TestHandler);
    let app = test::init_service(App::new().configure(router.configure_app())).await;

    // Initialize first (#153 PR 0b: only initialize may omit mcp-session-id).
    let req = test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("content-type", "application/json"))
        .insert_header(("mcp-protocol-version", "2025-11-25"))
        .set_payload(r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":0}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .expect("initialize must assign a session id");

    let req = test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("content-type", "application/json"))
        .insert_header(("mcp-protocol-version", "2025-11-25"))
        .insert_header(("mcp-session-id", sid.as_str()))
        .set_payload(r#"{"jsonrpc":"2.0","id":42,"result":{"roots":[]}}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::ACCEPTED);
}

/// #153 PR 0b: a non-initialize message without `mcp-session-id` is 400.
#[actix_rt::test]
async fn missing_session_id_on_non_initialize_is_400() {
    let router = McpRouter::new(TestHandler);
    let app = test::init_service(App::new().configure(router.configure_app())).await;

    let req = test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("content-type", "application/json"))
        .insert_header(("mcp-protocol-version", "2025-11-25"))
        .set_payload(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
}

/// An id the session's task store does not own is *invalid params* (-32602),
/// not *method not found* (-32601). The adapters have no custom task handler,
/// so a store that declines an id has declined it for good.
#[actix_rt::test]
async fn unknown_task_id_is_invalid_params_not_method_not_found() {
    let router = McpRouter::new(TestHandler);
    let app = test::init_service(App::new().configure(router.configure_app())).await;

    let req = test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("content-type", "application/json"))
        .insert_header(("mcp-protocol-version", "2025-11-25"))
        .set_payload(r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":0}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .expect("initialize must assign a session id");

    for method in ["tasks/get", "tasks/result", "tasks/cancel"] {
        let req = test::TestRequest::post()
            .uri("/mcp")
            .insert_header(("content-type", "application/json"))
            .insert_header(("mcp-protocol-version", "2025-11-25"))
            .insert_header(("mcp-session-id", sid.as_str()))
            .set_payload(format!(
                r#"{{"jsonrpc":"2.0","method":"{method}","params":{{"taskId":"no-such-task-id"}},"id":1}}"#
            ))
            .to_request();
        let json: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(
            json["error"]["code"], -32602,
            "{method} with an unknown id must be -32602: {json}"
        );
    }
}
