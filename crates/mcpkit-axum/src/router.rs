//! Router builder for MCP endpoints.

use crate::handler::{handle_mcp_post, handle_oauth_protected_resource, handle_sse};
use crate::state::{HasServerInfo, McpState, OAuthState};
use axum::Router;
use axum::routing::{get, post};
use mcpkit_core::auth::ProtectedResourceMetadata;
use mcpkit_server::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use mcpkit_transport::http::OriginValidator;
use std::sync::Arc;
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
    oauth_metadata: Option<ProtectedResourceMetadata>,
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
            post_path: "/mcp".to_string(),
            sse_path: "/mcp/sse".to_string(),
            oauth_metadata: None,
        }
    }

    /// Set the page size for `*/list` results. `None` (the default) disables
    /// pagination; a size of `0` is treated as disabled.
    #[must_use]
    pub const fn list_page_size(mut self, page_size: usize) -> Self {
        self.state.list_page_size = Some(page_size);
        self
    }

    /// Enable CORS with permissive defaults.
    ///
    /// For production, you should use `with_cors_layer` with a custom configuration.
    #[must_use]
    pub const fn with_cors(mut self) -> Self {
        self.enable_cors = true;
        self
    }

    /// Restrict which browser `Origin`s are accepted, for DNS-rebinding
    /// protection.
    ///
    /// Loopback origins (`localhost`, `127.0.0.1`, `[::1]`) are always allowed;
    /// the given origins (e.g. `https://app.example.com`) are added to the
    /// allow-list. Requests with no `Origin` header (non-browser clients) are
    /// allowed. By default — without calling this — only loopback origins are
    /// accepted.
    #[must_use]
    pub fn with_allowed_origins<I, S>(mut self, origins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut validator = OriginValidator::allow_list();
        for origin in origins {
            validator = validator.allow(origin);
        }
        self.state.origin_validator = Arc::new(validator);
        self
    }

    /// Disable `Origin` validation entirely, accepting every origin.
    ///
    /// **Insecure**: this removes DNS-rebinding protection. Only use it behind
    /// other safeguards (mTLS, a trusted network, authenticated sessions).
    #[must_use]
    pub fn allow_any_origin(mut self) -> Self {
        self.state.origin_validator = Arc::new(OriginValidator::allow_any());
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

    /// Enable OAuth 2.1 Protected Resource Metadata discovery.
    ///
    /// When enabled, the router will serve metadata at `/.well-known/oauth-protected-resource`
    /// per RFC 9728. This is required by the MCP specification for servers that require
    /// authentication.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The protected resource metadata to serve
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcpkit_axum::McpRouter;
    /// use mcpkit_core::auth::ProtectedResourceMetadata;
    ///
    /// let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
    ///     .with_authorization_server("https://auth.example.com")
    ///     .with_scopes(["files:read", "files:write"]);
    ///
    /// let router = McpRouter::new(MyHandler)
    ///     .with_oauth(metadata)
    ///     .into_router();
    /// ```
    #[must_use]
    pub fn with_oauth(mut self, metadata: ProtectedResourceMetadata) -> Self {
        self.oauth_metadata = Some(metadata);
        self
    }

    /// Build the router.
    pub fn into_router(self) -> Router {
        let mut router = Router::new()
            .route(&self.post_path, post(handle_mcp_post::<H>))
            .route(&self.sse_path, get(handle_sse::<H>))
            .with_state(self.state);

        // Add OAuth discovery endpoint if configured
        if let Some(metadata) = self.oauth_metadata {
            let oauth_router = Router::new()
                .route(
                    "/.well-known/oauth-protected-resource",
                    get(handle_oauth_protected_resource),
                )
                .with_state(OAuthState::new(metadata));
            router = router.merge(oauth_router);
        }

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
    use mcpkit_core::types::{
        GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput,
    };
    use mcpkit_server::ServerHandler;
    use mcpkit_server::context::Context;

    // Note: Clone is NOT required - the handler is wrapped in Arc internally
    struct TestHandler;

    impl ServerHandler for TestHandler {
        fn server_info(&self) -> ServerInfo {
            ServerInfo {
                name: "test-server".to_string(),
                title: None,
                version: "1.0.0".to_string(),
                protocol_version: None,
                icons: None,
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
                meta: None,
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

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `oneshot`

    fn post_with_origin(origin: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("mcp-protocol-version", "2025-06-18");
        if let Some(o) = origin {
            builder = builder.header("origin", o);
        }
        builder
            .body(Body::from(r#"{"jsonrpc":"2.0","method":"ping","id":1}"#))
            .unwrap()
    }

    #[tokio::test]
    async fn rejects_external_origin_by_default() {
        let router = McpRouter::new(TestHandler).into_router();
        let resp = router
            .oneshot(post_with_origin(Some("https://evil.example.com")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn allows_loopback_and_missing_origin_by_default() {
        let router = McpRouter::new(TestHandler).into_router();
        for origin in [
            Some("http://localhost:3000"),
            Some("http://127.0.0.1"),
            None,
        ] {
            let resp = router
                .clone()
                .oneshot(post_with_origin(origin))
                .await
                .unwrap();
            assert_ne!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "origin {origin:?} should pass the origin gate"
            );
        }
    }

    #[tokio::test]
    async fn with_allowed_origins_permits_only_configured() {
        let router = McpRouter::new(TestHandler)
            .with_allowed_origins(["https://app.example.com"])
            .into_router();

        let ok = router
            .clone()
            .oneshot(post_with_origin(Some("https://app.example.com")))
            .await
            .unwrap();
        assert_ne!(ok.status(), StatusCode::FORBIDDEN);

        let blocked = router
            .oneshot(post_with_origin(Some("https://other.example.com")))
            .await
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn allow_any_origin_accepts_external() {
        let router = McpRouter::new(TestHandler).allow_any_origin().into_router();
        let resp = router
            .oneshot(post_with_origin(Some("https://evil.example.com")))
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::FORBIDDEN);
    }
}
