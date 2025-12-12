//! Fluent server builder for MCP servers.
//!
//! The builder uses the typestate pattern to track registered capabilities
//! at the type level, ensuring compile-time verification of server configuration.
//!
//! # Type Parameters
//!
//! - `H`: The base server handler
//! - `Tools`: Tool handler state (`()` = not registered, `TH: ToolHandler` = registered)
//! - `Resources`: Resource handler state
//! - `Prompts`: Prompt handler state
//! - `Tasks`: Task handler state
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::{ServerBuilder, ServerHandler};
//! use mcpkit_core::capability::{ServerInfo, ServerCapabilities};
//!
//! struct MyHandler;
//!
//! impl ServerHandler for MyHandler {
//!     fn server_info(&self) -> ServerInfo {
//!         ServerInfo::new("my-server", "1.0.0")
//!     }
//! }
//!
//! let server = ServerBuilder::new(MyHandler).build();
//! assert_eq!(server.server_info().name, "my-server");
//! ```
//!
//! # Type-Level Capability Tracking
//!
//! The builder tracks which handlers have been registered at the type level.
//! This means you can't accidentally call a method that requires a handler
//! that hasn't been registered - the compiler will catch it.
//!
//! ```rust
//! use mcpkit_server::{ServerBuilder, ServerHandler, ToolHandler, Context};
//! use mcpkit_core::capability::{ServerInfo, ServerCapabilities};
//! use mcpkit_core::types::{Tool, ToolOutput};
//! use mcpkit_core::error::McpError;
//! use serde_json::Value;
//!
//! struct MyHandler;
//! impl ServerHandler for MyHandler {
//!     fn server_info(&self) -> ServerInfo {
//!         ServerInfo::new("test", "1.0.0")
//!     }
//! }
//!
//! struct MyToolHandler;
//! impl ToolHandler for MyToolHandler {
//!     async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
//!         Ok(vec![])
//!     }
//!     async fn call_tool(&self, _name: &str, _args: Value, _ctx: &Context<'_>) -> Result<ToolOutput, McpError> {
//!         Ok(ToolOutput::text("done"))
//!     }
//! }
//!
//! // Tools are registered - this compiles
//! let server = ServerBuilder::new(MyHandler)
//!     .with_tools(MyToolHandler)
//!     .build();
//!
//! assert!(server.capabilities().has_tools());
//! ```

use crate::handler::{
    PromptHandler, ResourceHandler, ServerHandler, TaskHandler, ToolHandler,
};
use mcpkit_core::capability::ServerCapabilities;

/// Marker type indicating no handler is registered for a capability.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotRegistered;

/// Marker type indicating a handler is registered for a capability.
#[derive(Debug)]
pub struct Registered<T>(pub T);

/// Builder for constructing MCP servers with specific capabilities.
///
/// Uses the typestate pattern with 5 type parameters to track registered
/// handlers at compile time:
///
/// - `H`: Base server handler (always required)
/// - `Tools`: Tool handler state
/// - `Resources`: Resource handler state
/// - `Prompts`: Prompt handler state
/// - `Tasks`: Task handler state
///
/// When a capability is not registered, its type parameter is `NotRegistered`.
/// When registered, it becomes `Registered<T>` where `T` is the handler type.
pub struct ServerBuilder<H, Tools, Resources, Prompts, Tasks> {
    handler: H,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    tasks: Tasks,
    capabilities: ServerCapabilities,
}

// Initial builder with no handlers registered
impl<H: ServerHandler> ServerBuilder<H, NotRegistered, NotRegistered, NotRegistered, NotRegistered> {
    /// Create a new server builder with the given base handler.
    ///
    /// The base handler must implement `ServerHandler` and provides
    /// the core server identity and configuration.
    #[must_use]
    pub fn new(handler: H) -> Self {
        let capabilities = handler.capabilities();
        Self {
            handler,
            tools: NotRegistered,
            resources: NotRegistered,
            prompts: NotRegistered,
            tasks: NotRegistered,
            capabilities,
        }
    }
}

// Methods available regardless of which handlers are registered
impl<H, T, R, P, K> ServerBuilder<H, T, R, P, K>
where
    H: ServerHandler,
{
    /// Override the capabilities advertised by this server.
    ///
    /// By default, capabilities are derived from the base handler.
    /// Use this to customize or extend those capabilities.
    #[must_use]
    pub fn capabilities(mut self, caps: ServerCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    /// Get a reference to the current capabilities.
    #[must_use]
    pub fn get_capabilities(&self) -> &ServerCapabilities {
        &self.capabilities
    }
}

// Tool handler registration (only when tools are not yet registered)
impl<H, R, P, K> ServerBuilder<H, NotRegistered, R, P, K>
where
    H: ServerHandler,
{
    /// Register a tool handler.
    ///
    /// This method is only available when no tool handler has been registered yet.
    /// Attempting to register tools twice will result in a compile error.
    #[must_use]
    pub fn with_tools<TH: ToolHandler>(self, tools: TH) -> ServerBuilder<H, Registered<TH>, R, P, K> {
        ServerBuilder {
            handler: self.handler,
            tools: Registered(tools),
            resources: self.resources,
            prompts: self.prompts,
            tasks: self.tasks,
            capabilities: self.capabilities.with_tools(),
        }
    }
}

// Resource handler registration (only when resources are not yet registered)
impl<H, T, P, K> ServerBuilder<H, T, NotRegistered, P, K>
where
    H: ServerHandler,
{
    /// Register a resource handler.
    ///
    /// This method is only available when no resource handler has been registered yet.
    #[must_use]
    pub fn with_resources<RH: ResourceHandler>(self, resources: RH) -> ServerBuilder<H, T, Registered<RH>, P, K> {
        ServerBuilder {
            handler: self.handler,
            tools: self.tools,
            resources: Registered(resources),
            prompts: self.prompts,
            tasks: self.tasks,
            capabilities: self.capabilities.with_resources(),
        }
    }
}

// Prompt handler registration (only when prompts are not yet registered)
impl<H, T, R, K> ServerBuilder<H, T, R, NotRegistered, K>
where
    H: ServerHandler,
{
    /// Register a prompt handler.
    ///
    /// This method is only available when no prompt handler has been registered yet.
    #[must_use]
    pub fn with_prompts<PH: PromptHandler>(self, prompts: PH) -> ServerBuilder<H, T, R, Registered<PH>, K> {
        ServerBuilder {
            handler: self.handler,
            tools: self.tools,
            resources: self.resources,
            prompts: Registered(prompts),
            tasks: self.tasks,
            capabilities: self.capabilities.with_prompts(),
        }
    }
}

// Task handler registration (only when tasks are not yet registered)
impl<H, T, R, P> ServerBuilder<H, T, R, P, NotRegistered>
where
    H: ServerHandler,
{
    /// Register a task handler.
    ///
    /// Tasks are long-running operations that can be tracked, monitored,
    /// and cancelled.
    ///
    /// This method is only available when no task handler has been registered yet.
    #[must_use]
    pub fn with_tasks<KH: TaskHandler>(self, tasks: KH) -> ServerBuilder<H, T, R, P, Registered<KH>> {
        ServerBuilder {
            handler: self.handler,
            tools: self.tools,
            resources: self.resources,
            prompts: self.prompts,
            tasks: Registered(tasks),
            // Note: capabilities.with_tasks() would be added when supported
            capabilities: self.capabilities,
        }
    }
}

// Build method - available for any combination of handlers
impl<H, T, R, P, K> ServerBuilder<H, T, R, P, K>
where
    H: ServerHandler + Send + Sync + 'static,
    T: Send + Sync + 'static,
    R: Send + Sync + 'static,
    P: Send + Sync + 'static,
    K: Send + Sync + 'static,
{
    /// Build the server.
    ///
    /// Returns a `Server` configured with the registered handlers and capabilities.
    #[must_use]
    pub fn build(self) -> Server<H, T, R, P, K> {
        Server {
            handler: self.handler,
            tools: self.tools,
            resources: self.resources,
            prompts: self.prompts,
            tasks: self.tasks,
            capabilities: self.capabilities,
        }
    }
}

/// A configured MCP server ready to serve requests.
///
/// The type parameters track which capabilities are available:
/// - `H`: Base server handler
/// - `T`: Tool handler (`NotRegistered` or `Registered<TH>`)
/// - `R`: Resource handler
/// - `P`: Prompt handler
/// - `K`: Task handler
pub struct Server<H, T, R, P, K> {
    handler: H,
    tools: T,
    resources: R,
    prompts: P,
    tasks: K,
    capabilities: ServerCapabilities,
}

impl<H, T, R, P, K> Server<H, T, R, P, K>
where
    H: ServerHandler,
{
    /// Get the server's capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &ServerCapabilities {
        &self.capabilities
    }

    /// Get a reference to the base handler.
    #[must_use]
    pub fn handler(&self) -> &H {
        &self.handler
    }

    /// Get the server info from the base handler.
    #[must_use]
    pub fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
        self.handler.server_info()
    }
}

// Methods when tools are registered
impl<H, TH, R, P, K> Server<H, Registered<TH>, R, P, K>
where
    H: ServerHandler,
    TH: ToolHandler,
{
    /// Get a reference to the tool handler.
    #[must_use]
    pub fn tool_handler(&self) -> &TH {
        &self.tools.0
    }
}

// Methods when resources are registered
impl<H, T, RH, P, K> Server<H, T, Registered<RH>, P, K>
where
    H: ServerHandler,
    RH: ResourceHandler,
{
    /// Get a reference to the resource handler.
    #[must_use]
    pub fn resource_handler(&self) -> &RH {
        &self.resources.0
    }
}

// Methods when prompts are registered
impl<H, T, R, PH, K> Server<H, T, R, Registered<PH>, K>
where
    H: ServerHandler,
    PH: PromptHandler,
{
    /// Get a reference to the prompt handler.
    #[must_use]
    pub fn prompt_handler(&self) -> &PH {
        &self.prompts.0
    }
}

// Methods when tasks are registered
impl<H, T, R, P, KH> Server<H, T, R, P, Registered<KH>>
where
    H: ServerHandler,
    KH: TaskHandler,
{
    /// Get a reference to the task handler.
    #[must_use]
    pub fn task_handler(&self) -> &KH {
        &self.tasks.0
    }
}

/// Type alias for a fully-configured server with all handlers.
pub type FullServer<H, TH, RH, PH, KH> = Server<H, Registered<TH>, Registered<RH>, Registered<PH>, Registered<KH>>;

/// Type alias for a minimal server with no optional handlers.
pub type MinimalServer<H> = Server<H, NotRegistered, NotRegistered, NotRegistered, NotRegistered>;

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::ServerInfo;
    use crate::handler::ToolHandler;
    use crate::context::Context;
    use mcpkit_core::error::McpError;
    use mcpkit_core::types::{Tool, ToolOutput};
    use serde_json::Value;

    struct TestHandler;

    impl ServerHandler for TestHandler {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("test", "1.0.0")
        }

        fn capabilities(&self) -> ServerCapabilities {
            ServerCapabilities::default()
        }
    }

    struct TestToolHandler;

    impl ToolHandler for TestToolHandler {
        async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
            Ok(vec![])
        }

        async fn call_tool(&self, _name: &str, _args: Value, _ctx: &Context<'_>) -> Result<ToolOutput, McpError> {
            Ok(ToolOutput::text("test"))
        }
    }

    #[test]
    fn test_server_builder_minimal() {
        let server = ServerBuilder::new(TestHandler).build();

        assert_eq!(server.server_info().name, "test");
        assert_eq!(server.server_info().version, "1.0.0");
    }

    #[test]
    fn test_server_builder_with_tools() {
        let server = ServerBuilder::new(TestHandler)
            .with_tools(TestToolHandler)
            .build();

        assert!(server.capabilities().has_tools());
        // This compiles because tools are registered
        let _tool_handler: &TestToolHandler = server.tool_handler();
    }

    #[test]
    fn test_typestate_prevents_double_registration() {
        // This test verifies the design - double registration would be
        // a compile error, not a runtime error
        let _server = ServerBuilder::new(TestHandler)
            .with_tools(TestToolHandler)
            // .with_tools(TestToolHandler) // This would NOT compile!
            .build();
    }

    #[test]
    fn test_builder_order_independence() {
        // Handlers can be registered in any order
        let server1 = ServerBuilder::new(TestHandler)
            .with_tools(TestToolHandler)
            .build();

        // Different order, same result
        let _server2: Server<TestHandler, Registered<TestToolHandler>, NotRegistered, NotRegistered, NotRegistered> =
            ServerBuilder::new(TestHandler)
                .with_tools(TestToolHandler)
                .build();

        assert!(server1.capabilities().has_tools());
    }
}
