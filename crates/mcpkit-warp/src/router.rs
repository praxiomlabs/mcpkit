//! Router builder for MCP endpoints in Warp.

use crate::handler::{handle_mcp_post, handle_sse, with_protocol_version, with_session_id};
use crate::state::{HasServerInfo, McpState};
use mcpkit_server::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use warp::Filter;

/// Builder for MCP Warp routers.
///
/// Creates a pre-configured Warp filter with MCP endpoints.
///
/// # Example
///
/// ```ignore
/// use mcpkit_warp::McpRouter;
///
/// struct MyHandler;
///
/// // Basic usage - serve the MCP server
/// #[tokio::main]
/// async fn main() {
///     McpRouter::new(MyHandler)
///         .serve(([0, 0, 0, 0], 3000))
///         .await;
/// }
/// ```
pub struct McpRouter<H> {
    state: Arc<McpState<H>>,
    enable_cors: bool,
}

impl<H> McpRouter<H>
where
    H: ServerHandler
        + ToolHandler
        + ResourceHandler
        + PromptHandler
        + HasServerInfo
        + Send
        + Sync
        + 'static,
{
    /// Create a new MCP router with the given handler.
    pub fn new(handler: H) -> Self {
        Self {
            state: Arc::new(McpState::new(handler)),
            enable_cors: false,
        }
    }

    /// Enable CORS with permissive defaults.
    #[must_use]
    pub const fn with_cors(mut self) -> Self {
        self.enable_cors = true;
        self
    }

    /// Build the Warp filter for MCP endpoints with CORS enabled.
    ///
    /// Returns a filter that can be combined with other Warp filters.
    /// CORS is applied with permissive defaults suitable for development.
    /// For production, consider using `into_filter_without_cors()` and
    /// applying your own CORS configuration.
    #[must_use]
    pub fn into_filter(
        self,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        let state = self.state;

        // POST /mcp - Handle JSON-RPC requests
        let post_state = state.clone();
        let mcp_post = warp::path("mcp")
            .and(warp::post())
            .and(with_state(post_state))
            .and(with_protocol_version())
            .and(with_session_id())
            .and(warp::body::content_length_limit(1024 * 1024)) // 1MB limit
            .and(warp::body::bytes())
            .and_then(
                |state: Arc<McpState<H>>,
                 version: Option<String>,
                 session_id: Option<String>,
                 bytes: bytes::Bytes| async move {
                    let body = String::from_utf8_lossy(&bytes).to_string();
                    handle_mcp_post(state, version, session_id, body).await
                },
            );

        // GET /mcp/sse - Server-Sent Events
        let sse_state = state;
        let mcp_sse = warp::path("mcp")
            .and(warp::path("sse"))
            .and(warp::get())
            .and(with_state(sse_state))
            .and(with_session_id())
            .map(|state: Arc<McpState<H>>, session_id: Option<String>| {
                handle_sse(state, session_id)
            });

        // Combine routes with CORS
        mcp_post.or(mcp_sse).with(
            warp::cors()
                .allow_any_origin()
                .allow_methods(vec!["GET", "POST", "OPTIONS"])
                .allow_headers(vec![
                    "content-type",
                    "mcp-protocol-version",
                    "mcp-session-id",
                    "last-event-id",
                ])
                .expose_headers(vec!["mcp-session-id"]),
        )
    }

    /// Build the Warp filter for MCP endpoints (without CORS).
    ///
    /// This is useful when you want to add your own CORS configuration.
    #[must_use]
    pub fn into_filter_without_cors(
        self,
    ) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
        let state = self.state;

        // POST /mcp - Handle JSON-RPC requests
        let post_state = state.clone();
        let mcp_post = warp::path("mcp")
            .and(warp::post())
            .and(with_state(post_state))
            .and(with_protocol_version())
            .and(with_session_id())
            .and(warp::body::content_length_limit(1024 * 1024)) // 1MB limit
            .and(warp::body::bytes())
            .and_then(
                |state: Arc<McpState<H>>,
                 version: Option<String>,
                 session_id: Option<String>,
                 bytes: bytes::Bytes| async move {
                    let body = String::from_utf8_lossy(&bytes).to_string();
                    handle_mcp_post(state, version, session_id, body).await
                },
            );

        // GET /mcp/sse - Server-Sent Events
        let sse_state = state;
        let mcp_sse = warp::path("mcp")
            .and(warp::path("sse"))
            .and(warp::get())
            .and(with_state(sse_state))
            .and(with_session_id())
            .map(|state: Arc<McpState<H>>, session_id: Option<String>| {
                handle_sse(state, session_id)
            });

        mcp_post.or(mcp_sse)
    }

    /// Serve the MCP server on the given address.
    ///
    /// This is a convenience method that provides a stdio-like experience:
    ///
    /// ```ignore
    /// // stdio pattern:
    /// handler.into_server().serve(transport).await?;
    ///
    /// // warp pattern (now similar):
    /// McpRouter::new(handler).serve(([0, 0, 0, 0], 3000)).await;
    /// ```
    pub async fn serve(self, addr: impl Into<SocketAddr>) {
        let filter = self.into_filter();
        warp::serve(filter).run(addr).await;
    }
}

/// Create a filter that provides the MCP state.
fn with_state<H: Send + Sync + 'static>(
    state: Arc<McpState<H>>,
) -> impl Filter<Extract = (Arc<McpState<H>>,), Error = Infallible> + Clone {
    warp::any().map(move || state.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
    use mcpkit_core::error::McpError;
    use mcpkit_core::types::{
        GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput,
    };
    use mcpkit_server::ServerHandler;
    use mcpkit_server::context::Context;

    struct TestHandler;

    impl ServerHandler for TestHandler {
        fn server_info(&self) -> ServerInfo {
            ServerInfo {
                name: "test-server".to_string(),
                version: "1.0.0".to_string(),
                protocol_version: None,
            }
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
            Ok(vec![])
        }

        async fn call_tool(
            &self,
            _name: &str,
            _args: serde_json::Value,
            _ctx: &Context<'_>,
        ) -> Result<ToolOutput, McpError> {
            Ok(ToolOutput::text("test"))
        }
    }

    impl ResourceHandler for TestHandler {
        async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
            Ok(vec![])
        }

        async fn read_resource(
            &self,
            uri: &str,
            _ctx: &Context<'_>,
        ) -> Result<Vec<ResourceContents>, McpError> {
            Ok(vec![ResourceContents::text(uri, "test")])
        }
    }

    impl PromptHandler for TestHandler {
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
                description: Some("Test prompt".to_string()),
                messages: vec![],
            })
        }
    }

    #[test]
    fn test_router_builder() {
        let router = McpRouter::new(TestHandler).with_cors();

        // Router should be created without panicking
        let _ = router.into_filter();
    }
}
