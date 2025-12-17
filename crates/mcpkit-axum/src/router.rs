//! Router builder for MCP endpoints.

use crate::handler::{handle_mcp_post, handle_sse};
use crate::state::{HasServerInfo, McpState};
use axum::Router;
use axum::routing::{get, post};
use mcpkit_server::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Builder for MCP Axum routers.
///
/// Creates a pre-configured Axum router with MCP endpoints.
///
/// # Example
///
/// ```ignore
/// use mcpkit_axum::McpRouter;
///
/// struct MyHandler;
///
/// // Basic usage
/// let router = McpRouter::new(MyHandler).into_router();
///
/// // With CORS
/// let router = McpRouter::new(MyHandler)
///     .with_cors()
///     .into_router();
///
/// // With tracing
/// let router = McpRouter::new(MyHandler)
///     .with_tracing()
///     .into_router();
/// ```
pub struct McpRouter<H> {
    state: McpState<H>,
    enable_cors: bool,
    enable_tracing: bool,
    post_path: String,
    sse_path: String,
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
            state: McpState::new(handler),
            enable_cors: false,
            enable_tracing: false,
            post_path: "/".to_string(),
            sse_path: "/sse".to_string(),
        }
    }

    /// Enable CORS with permissive defaults.
    ///
    /// For production, you should use `with_cors_layer` with a custom configuration.
    #[must_use]
    pub const fn with_cors(mut self) -> Self {
        self.enable_cors = true;
        self
    }

    /// Enable request tracing.
    #[must_use]
    pub const fn with_tracing(mut self) -> Self {
        self.enable_tracing = true;
        self
    }

    /// Set the path for POST requests.
    #[must_use]
    pub fn post_path(mut self, path: impl Into<String>) -> Self {
        self.post_path = path.into();
        self
    }

    /// Set the path for SSE connections.
    #[must_use]
    pub fn sse_path(mut self, path: impl Into<String>) -> Self {
        self.sse_path = path.into();
        self
    }

    /// Build the router.
    #[must_use]
    pub fn into_router(self) -> Router {
        let mut router = Router::new()
            .route(&self.post_path, post(handle_mcp_post::<H>))
            .route(&self.sse_path, get(handle_sse::<H>))
            .with_state(self.state);

        if self.enable_cors {
            router = router.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        if self.enable_tracing {
            router = router.layer(TraceLayer::new_for_http());
        }

        router
    }

    /// Serve the MCP server on the given address.
    ///
    /// This is a convenience method that provides a stdio-like experience:
    ///
    /// ```ignore
    /// // stdio pattern:
    /// handler.into_server().serve(transport).await?;
    ///
    /// // http pattern (now similar):
    /// McpRouter::new(handler).serve("0.0.0.0:3000").await?;
    /// ```
    ///
    /// For more control over the server, use [`Self::into_router`] instead.
    pub async fn serve(self, addr: &str) -> std::io::Result<()> {
        let router = self.into_router();
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router)
            .await
            .map_err(std::io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
    use mcpkit_core::error::McpError;
    use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
    use mcpkit_server::context::Context;
    use mcpkit_server::ServerHandler;

    // Note: Clone is NOT required - the handler is wrapped in Arc internally
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
            ServerCapabilities::new().with_tools().with_resources().with_prompts()
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
        let router = McpRouter::new(TestHandler)
            .with_cors()
            .with_tracing()
            .post_path("/api/mcp")
            .sse_path("/api/sse")
            .into_router();

        // Router should be created without panicking
        let _ = router;
    }
}
