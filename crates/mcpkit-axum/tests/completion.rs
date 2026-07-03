//! Regression test for #113: `completion/complete` must be answered through the
//! HTTP adapter (with a registered completion handler), not return
//! method-not-found. The adapters dispatch via the shared `route_completion`
//! helper, so this guards the adapter "routing split" from forgetting completion.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use mcpkit_axum::McpState;
use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    CompleteRequest, Completion, GetPromptResult, Prompt, Resource, ResourceContents, Tool,
    ToolOutput,
};
use mcpkit_server::{
    CompletionHandler, Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler,
};

struct H;

impl ServerHandler for H {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("t", "1.0.0")
    }
    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new().with_completions()
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

/// Completion handler that echoes the previously-resolved `owner` context value.
struct Comp;
impl CompletionHandler for Comp {
    async fn complete(
        &self,
        request: &CompleteRequest,
        _ctx: &Context<'_>,
    ) -> Result<Completion, McpError> {
        let owner = request
            .context
            .as_ref()
            .and_then(|c| c.arguments.as_ref())
            .and_then(|a| a.get("owner").cloned())
            .unwrap_or_default();
        Ok(Completion::new(vec![format!("{owner}-suggestion")]))
    }
}

#[tokio::test]
async fn completion_complete_works_through_axum_adapter() {
    let state = McpState::new(H).with_completion(Comp);
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "completion/complete",
        "params": {
            "ref": { "type": "ref/prompt", "name": "p" },
            "argument": { "name": "a", "value": "x" },
            "context": { "arguments": { "owner": "acme" } }
        }
    })
    .to_string();

    let response = mcpkit_axum::handle_mcp_post(State(state), HeaderMap::new(), None, body)
        .await
        .into_response();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    // Answered (not method-not-found), and the completion context propagated.
    assert!(json.get("error").is_none(), "response: {json}");
    assert_eq!(
        json["result"]["completion"]["values"][0], "acme-suggestion",
        "response: {json}"
    );
}

#[tokio::test]
async fn completion_complete_without_handler_is_method_not_found() {
    // Without `.with_completion(..)`, the adapter must report method-not-found
    // rather than silently succeeding.
    let state = McpState::new(H);
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "completion/complete",
        "params": {
            "ref": { "type": "ref/prompt", "name": "p" },
            "argument": { "name": "a", "value": "x" }
        }
    })
    .to_string();

    let response = mcpkit_axum::handle_mcp_post(State(state), HeaderMap::new(), None, body)
        .await
        .into_response();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    assert_eq!(json["error"]["code"], -32601, "response: {json}");
}
