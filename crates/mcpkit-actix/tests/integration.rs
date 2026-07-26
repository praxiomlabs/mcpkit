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

    let req = test::TestRequest::post()
        .uri("/mcp")
        .insert_header(("content-type", "application/json"))
        .insert_header(("mcp-protocol-version", "2025-11-25"))
        .set_payload(r#"{"jsonrpc":"2.0","id":42,"result":{"roots":[]}}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), actix_web::http::StatusCode::ACCEPTED);
}
