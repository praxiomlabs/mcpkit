//! Router builder for MCP endpoints.

use crate::handler::{handle_mcp_post, handle_sse};
use crate::state::{HasServerInfo, McpState};
use axum::routing::{get, post};
use axum::Router;
use mcpkit_server::ServerHandler;
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
    H: ServerHandler + HasServerInfo + Send + Sync + Clone + 'static,
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
    pub fn with_cors(mut self) -> Self {
        self.enable_cors = true;
        self
    }

    /// Enable request tracing.
    #[must_use]
    pub fn with_tracing(mut self) -> Self {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
    use mcpkit_server::ServerHandler;

    #[derive(Clone)]
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
            ServerCapabilities::default()
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
