//! Router builder for MCP endpoints.

use crate::handler::{handle_mcp_post, handle_oauth_protected_resource, handle_sse};
use crate::state::{HasServerInfo, McpState, OAuthState};
use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{App, HttpServer, web};
use mcpkit_core::auth::ProtectedResourceMetadata;
use mcpkit_server::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};

/// Builder for MCP Actix routers.
///
/// Creates a pre-configured Actix web application with MCP endpoints.
///
/// # Example
///
/// ```ignore
/// use mcpkit_actix::McpRouter;
///
/// struct MyHandler;
///
/// // Basic usage with serve (similar to mcpkit-axum)
/// McpRouter::new(MyHandler)
///     .serve("0.0.0.0:3000")
///     .await?;
///
/// // With CORS
/// McpRouter::new(MyHandler)
///     .with_cors()
///     .serve("0.0.0.0:3000")
///     .await?;
///
/// // With logging
/// McpRouter::new(MyHandler)
///     .with_logging()
///     .serve("0.0.0.0:3000")
///     .await?;
/// ```
pub struct McpRouter<H> {
    state: McpState<H>,
    enable_cors: bool,
    enable_logging: bool,
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
            enable_logging: false,
            post_path: "/mcp".to_string(),
            sse_path: "/mcp/sse".to_string(),
            oauth_metadata: None,
        }
    }

    /// Enable CORS with permissive defaults.
    ///
    /// For production, you should configure CORS manually with custom settings.
    #[must_use]
    pub const fn with_cors(mut self) -> Self {
        self.enable_cors = true;
        self
    }

    /// Enable request logging.
    #[must_use]
    pub const fn with_logging(mut self) -> Self {
        self.enable_logging = true;
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
    /// use mcpkit_actix::McpRouter;
    /// use mcpkit_core::auth::ProtectedResourceMetadata;
    ///
    /// let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
    ///     .with_authorization_server("https://auth.example.com")
    ///     .with_scopes(["files:read", "files:write"]);
    ///
    /// McpRouter::new(MyHandler)
    ///     .with_oauth(metadata)
    ///     .serve("0.0.0.0:3000")
    ///     .await?;
    /// ```
    #[must_use]
    pub fn with_oauth(mut self, metadata: ProtectedResourceMetadata) -> Self {
        self.oauth_metadata = Some(metadata);
        self
    }

    /// Configure an Actix App with MCP routes.
    ///
    /// This is useful when you need to integrate MCP routes with an existing Actix application.
    pub fn configure_app(&self) -> impl Fn(&mut web::ServiceConfig) + Clone + 'static {
        let state = self.state.clone();
        let post_path = self.post_path.clone();
        let sse_path = self.sse_path.clone();
        let oauth_metadata = self.oauth_metadata.clone();

        move |cfg: &mut web::ServiceConfig| {
            cfg.app_data(web::Data::new(state.clone()))
                .route(&post_path, web::post().to(handle_mcp_post::<H>))
                .route(&sse_path, web::get().to(handle_sse::<H>));

            // Add OAuth discovery endpoint if configured
            if let Some(metadata) = &oauth_metadata {
                cfg.app_data(web::Data::new(OAuthState::new(metadata.clone())))
                    .route(
                        "/.well-known/oauth-protected-resource",
                        web::get().to(handle_oauth_protected_resource),
                    );
            }
        }
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
    /// For more control over the server, use [`Self::configure_app`] instead.
    pub async fn serve(self, addr: &str) -> std::io::Result<()> {
        let enable_cors = self.enable_cors;
        let enable_logging = self.enable_logging;
        let configure = self.configure_app();

        // Due to Actix's type system, we need to handle middleware combinations explicitly
        match (enable_cors, enable_logging) {
            (true, true) => {
                HttpServer::new(move || {
                    App::new()
                        .configure(configure.clone())
                        .wrap(Cors::permissive())
                        .wrap(Logger::default())
                })
                .bind(addr)?
                .run()
                .await
            }
            (true, false) => {
                HttpServer::new(move || {
                    App::new()
                        .configure(configure.clone())
                        .wrap(Cors::permissive())
                })
                .bind(addr)?
                .run()
                .await
            }
            (false, true) => {
                HttpServer::new(move || {
                    App::new()
                        .configure(configure.clone())
                        .wrap(Logger::default())
                })
                .bind(addr)?
                .run()
                .await
            }
            (false, false) => {
                HttpServer::new(move || App::new().configure(configure.clone()))
                    .bind(addr)?
                    .run()
                    .await
            }
        }
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
        let router = McpRouter::new(TestHandler)
            .with_cors()
            .with_logging()
            .post_path("/api/mcp")
            .sse_path("/api/sse");

        // Router should be created without panicking
        let _ = router;
    }
}
