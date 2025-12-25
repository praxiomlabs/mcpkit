//! Router builder for MCP endpoints in Rocket.

use crate::state::{HasServerInfo, McpState};
use mcpkit_server::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::{Build, Request, Response, Rocket};

/// Builder for MCP Rocket routers.
///
/// Creates a pre-configured Rocket with MCP endpoints.
///
/// # Example
///
/// ```ignore
/// use mcpkit_rocket::McpRouter;
///
/// struct MyHandler;
///
/// // Basic usage - launch the server
/// #[rocket::main]
/// async fn main() -> Result<(), rocket::Error> {
///     McpRouter::new(MyHandler)
///         .launch()
///         .await?;
///     Ok(())
/// }
/// ```
pub struct McpRouter<H> {
    state: McpState<H>,
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
            state: McpState::new(handler),
            enable_cors: false,
        }
    }

    /// Enable CORS with permissive defaults.
    #[must_use]
    pub const fn with_cors(mut self) -> Self {
        self.enable_cors = true;
        self
    }

    /// Build a Rocket instance with MCP routes.
    ///
    /// Note: Due to Rocket's type system constraints, this method creates
    /// routes that are specific to the handler type. Use the `create_routes!`
    /// macro in your application to generate the routes.
    #[must_use] 
    pub fn into_rocket(self) -> Rocket<Build> {
        let mut rocket = rocket::build().manage(self.state);

        if self.enable_cors {
            rocket = rocket.attach(Cors);
        }

        rocket
    }

    /// Get the MCP state for use with custom route handlers.
    #[must_use] 
    pub fn into_state(self) -> McpState<H> {
        self.state
    }

    /// Launch the MCP server.
    ///
    /// This is a convenience method that provides a stdio-like experience.
    /// Note: You'll need to mount the routes separately using macros.
    pub async fn launch(self) -> Result<(), rocket::Error> {
        let _ = self.into_rocket().launch().await?;
        Ok(())
    }
}

/// CORS fairing for permissive cross-origin requests.
pub struct Cors;

#[rocket::async_trait]
impl Fairing for Cors {
    fn info(&self) -> Info {
        Info {
            name: "CORS",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new(
            "Access-Control-Allow-Methods",
            "GET, POST, OPTIONS",
        ));
        response.set_header(Header::new(
            "Access-Control-Allow-Headers",
            "Content-Type, mcp-protocol-version, mcp-session-id, last-event-id",
        ));
        response.set_header(Header::new("Access-Control-Expose-Headers", "mcp-session-id"));
    }
}

/// Create MCP route handlers for a specific handler type.
///
/// This macro generates the Rocket route handlers for your MCP server.
/// Due to Rocket's type system constraints, the route handlers must be
/// generated at compile time for your specific handler type.
///
/// # Example
///
/// ```ignore
/// use mcpkit_rocket::{McpRouter, create_mcp_routes};
///
/// struct MyHandler;
/// // ... implement ServerHandler, ToolHandler, etc. for MyHandler
///
/// // Generate the routes
/// create_mcp_routes!(MyHandler);
///
/// #[rocket::main]
/// async fn main() -> Result<(), rocket::Error> {
///     let state = McpRouter::new(MyHandler).into_state();
///
///     rocket::build()
///         .manage(state)
///         .mount("/", routes![mcp_post, mcp_sse])
///         .launch()
///         .await?;
///
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! create_mcp_routes {
    ($handler_type:ty) => {
        #[rocket::post("/mcp", data = "<body>")]
        async fn mcp_post(
            state: &::rocket::State<$crate::McpState<$handler_type>>,
            version: $crate::handler::ProtocolVersionHeader,
            session: $crate::handler::SessionIdHeader,
            body: String,
        ) -> $crate::handler::McpResponse {
            $crate::handler::handle_mcp_post(
                state.inner(),
                version.0.as_deref(),
                session.0,
                &body,
            )
            .await
        }

        #[rocket::get("/mcp/sse")]
        fn mcp_sse(
            state: &::rocket::State<$crate::McpState<$handler_type>>,
            session: $crate::handler::SessionIdHeader,
        ) -> ::rocket::response::stream::EventStream![] {
            $crate::handler::handle_sse(state.inner(), session.0)
        }
    };
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
        let _ = router.into_rocket();
    }

    #[test]
    fn test_state_extraction() {
        let router = McpRouter::new(TestHandler);
        let state = router.into_state();

        assert_eq!(state.server_info.name, "test-server");
        assert_eq!(state.server_info.version, "1.0.0");
    }
}
