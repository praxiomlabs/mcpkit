//! Regression test for #108: `logging/setLevel` must work through the HTTP
//! adapter, not just the stdio runtime router. The adapters dispatch by calling
//! the shared `route_*` helpers directly, so this guards against the adapter
//! "routing split" forgetting to route logging.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use mcpkit_axum::McpState;
use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    GetPromptResult, LoggingLevel, Prompt, Resource, ResourceContents, Tool, ToolOutput,
};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use std::sync::{Arc, Mutex};

struct H(Arc<Mutex<Option<LoggingLevel>>>);

impl ServerHandler for H {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("t", "1.0.0")
    }
    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new().with_logging()
    }
    async fn set_log_level(&self, level: LoggingLevel, _ctx: &Context<'_>) -> Result<(), McpError> {
        *self.0.lock().unwrap() = Some(level);
        Ok(())
    }
}
impl ToolHandler for H {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![])
    }
    async fn call_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        Ok(ToolOutput::text("x"))
    }
}
impl ResourceHandler for H {
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
impl PromptHandler for H {
    async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
        Ok(vec![])
    }
    async fn get_prompt(
        &self,
        _name: &str,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        Ok(GetPromptResult {
            description: None,
            messages: vec![],
            meta: None,
        })
    }
}

#[tokio::test]
async fn logging_set_level_works_through_axum_adapter() {
    let seen = Arc::new(Mutex::new(None));
    let state = McpState::new(H(Arc::clone(&seen)));
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "logging/setLevel",
        "params": { "level": "warning" }
    })
    .to_string();

    let response = mcpkit_axum::handle_mcp_post(State(state), HeaderMap::new(), None, body)
        .await
        .into_response();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    assert_eq!(json["result"], serde_json::json!({}), "response: {json}");
    assert_eq!(*seen.lock().unwrap(), Some(LoggingLevel::Warning));
}
