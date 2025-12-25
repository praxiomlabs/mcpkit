//! State management for MCP Rocket integration.

use crate::session::SessionStore;
use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_server::ServerHandler;
use std::sync::Arc;

/// Trait for handlers that provide server info.
pub trait HasServerInfo {
    /// Get the server info.
    fn server_info(&self) -> ServerInfo;
}

impl<H: ServerHandler> HasServerInfo for H {
    fn server_info(&self) -> ServerInfo {
        ServerHandler::server_info(self)
    }
}

/// Shared state for MCP request handling.
pub struct McpState<H> {
    /// The MCP handler implementation.
    pub handler: Arc<H>,
    /// Server info for initialization responses.
    pub server_info: ServerInfo,
    /// Session manager for tracking client sessions.
    pub sessions: SessionStore,
    /// SSE session manager for Server-Sent Events.
    pub sse_sessions: SessionStore,
}

impl<H> McpState<H>
where
    H: HasServerInfo,
{
    /// Create new MCP state.
    pub fn new(handler: H) -> Self {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            server_info,
            sessions: SessionStore::new(),
            sse_sessions: SessionStore::new(),
        }
    }

    /// Get the handler's capabilities.
    #[must_use] 
    pub fn capabilities(&self) -> ServerCapabilities
    where
        H: ServerHandler,
    {
        self.handler.capabilities()
    }
}

impl<H> Clone for McpState<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            server_info: self.server_info.clone(),
            sessions: self.sessions.clone(),
            sse_sessions: self.sse_sessions.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::error::McpError;
    use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
    use mcpkit_server::handler::{PromptHandler, ResourceHandler, ToolHandler};
    use mcpkit_server::context::Context;

    struct TestHandler;

    impl ServerHandler for TestHandler {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("test-server", "1.0.0")
        }

        fn capabilities(&self) -> ServerCapabilities {
            ServerCapabilities::new()
                .with_tools()
                .with_resources()
        }
    }

    impl ToolHandler for TestHandler {
        async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
            Ok(vec![Tool::new("test").description("A test tool")])
        }

        async fn call_tool(
            &self,
            _name: &str,
            _args: serde_json::Value,
            _ctx: &Context<'_>,
        ) -> Result<ToolOutput, McpError> {
            Ok(ToolOutput::text("test result"))
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
            Ok(vec![ResourceContents::text(uri, "content")])
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
                description: Some("Test".to_string()),
                messages: vec![],
            })
        }
    }

    #[test]
    fn test_mcp_state_creation() {
        let state = McpState::new(TestHandler);

        assert_eq!(state.server_info.name, "test-server");
        assert_eq!(state.server_info.version, "1.0.0");
    }

    #[test]
    fn test_mcp_state_capabilities() {
        let state = McpState::new(TestHandler);
        let caps = state.capabilities();

        assert!(caps.tools.is_some());
        assert!(caps.resources.is_some());
    }

    #[test]
    fn test_mcp_state_clone() {
        let state = McpState::new(TestHandler);
        let cloned = state.clone();

        assert_eq!(cloned.server_info.name, state.server_info.name);
        assert_eq!(cloned.server_info.version, state.server_info.version);
    }

    #[test]
    fn test_mcp_state_sessions() {
        let state = McpState::new(TestHandler);

        // Create sessions in both stores
        let id1 = state.sessions.create();
        let id2 = state.sse_sessions.create();

        assert!(state.sessions.exists(&id1));
        assert!(state.sse_sessions.exists(&id2));
    }

    #[test]
    fn test_has_server_info_trait() {
        let handler = TestHandler;
        let info = HasServerInfo::server_info(&handler);

        assert_eq!(info.name, "test-server");
        assert_eq!(info.version, "1.0.0");
    }
}
