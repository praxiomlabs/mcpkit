//! Composable handler traits for MCP servers.
//!
//! This module defines the traits that MCP servers can implement to handle
//! different protocol capabilities. Unlike monolithic handler approaches,
//! these traits are composable - implement only what you need.
//!
//! # Overview
//!
//! - [`ServerHandler`]: Minimal required trait for all servers
//! - [`ToolHandler`]: Handle tool discovery and execution
//! - [`ResourceHandler`]: Handle resource discovery and reading
//! - [`PromptHandler`]: Handle prompt discovery and rendering
//! - [`TaskHandler`]: Handle long-running task operations
//! - [`SamplingHandler`]: Handle sampling requests (client-to-server)
//! - [`ElicitationHandler`]: Handle elicitation requests (client-to-server)
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::{ServerHandler, ServerBuilder};
//! use mcpkit_core::capability::{ServerInfo, ServerCapabilities};
//!
//! struct MyServer;
//!
//! impl ServerHandler for MyServer {
//!     fn server_info(&self) -> ServerInfo {
//!         ServerInfo::new("my-server", "1.0.0")
//!     }
//!
//!     fn capabilities(&self) -> ServerCapabilities {
//!         ServerCapabilities::new().with_tools()
//!     }
//! }
//!
//! let server = ServerBuilder::new(MyServer).build();
//! assert_eq!(server.server_info().name, "my-server");
//! ```

use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    GetPromptResult, Prompt, Resource, ResourceContents, ResourceTemplate, Task, TaskId, Tool,
    ToolOutput,
    elicitation::{ElicitRequest, ElicitResult},
    sampling::{CreateMessageRequest, CreateMessageResult},
};
use serde_json::Value;
use std::future::Future;

use crate::context::Context;

/// Core server handler trait - required for all MCP servers.
///
/// This trait defines the minimal requirements for an MCP server.
/// All servers must implement this trait. Additional capabilities
/// are added by implementing optional handler traits.
///
/// Note: Context uses lifetime references (no `'static` requirement).
pub trait ServerHandler: Send + Sync {
    /// Return information about this server.
    ///
    /// This is called during the initialization handshake.
    fn server_info(&self) -> ServerInfo;

    /// Return the capabilities of this server.
    ///
    /// The default implementation returns empty capabilities.
    /// Override this to advertise specific capabilities.
    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::default()
    }

    /// Return optional instructions for using this server.
    ///
    /// These instructions are sent to the client during initialization
    /// and can help the AI assistant understand how to use this server.
    fn instructions(&self) -> Option<String> {
        None
    }

    /// Called after initialization is complete.
    ///
    /// This is a good place to set up any state that requires
    /// the connection to be established.
    fn on_initialized(&self, _ctx: &Context<'_>) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when the connection is about to be closed.
    fn on_shutdown(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// Handler for tool-related operations.
///
/// Implement this trait to expose tools that AI assistants can call.
pub trait ToolHandler: Send + Sync {
    /// List all available tools.
    ///
    /// This is called when the client requests the tool list.
    fn list_tools(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Tool>, McpError>> + Send;

    /// Call a tool with the given arguments.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to call
    /// * `args` - The arguments as a JSON value
    /// * `ctx` - The request context
    fn call_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<ToolOutput, McpError>> + Send;

    /// Called when a tool's definition has changed.
    ///
    /// Override this to dynamically add/remove/update tools.
    fn on_tools_changed(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// Handler for resource-related operations.
///
/// Implement this trait to expose resources that AI assistants can read.
pub trait ResourceHandler: Send + Sync {
    /// List all available static resources.
    ///
    /// This returns resources with fixed URIs. For dynamic resources with
    /// parameterized URIs (e.g., `file://{path}`), use `list_resource_templates()`.
    fn list_resources(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Resource>, McpError>> + Send;

    /// List all available resource templates.
    ///
    /// Resource templates describe dynamic resources with parameterized URIs.
    /// For example, a template with URI `file://{path}` allows clients to
    /// construct URIs like `file:///etc/hosts` to read specific files.
    ///
    /// The default implementation returns an empty list.
    fn list_resource_templates(
        &self,
        _ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<ResourceTemplate>, McpError>> + Send {
        async { Ok(vec![]) }
    }

    /// Read a resource by URI.
    fn read_resource(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<ResourceContents>, McpError>> + Send;

    /// Subscribe to resource updates.
    ///
    /// Returns true if the subscription was successful.
    fn subscribe(
        &self,
        _uri: &str,
        _ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        async { Ok(false) }
    }

    /// Unsubscribe from resource updates.
    fn unsubscribe(
        &self,
        _uri: &str,
        _ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        async { Ok(false) }
    }
}

/// Handler for prompt-related operations.
///
/// Implement this trait to expose prompts that AI assistants can use.
pub trait PromptHandler: Send + Sync {
    /// List all available prompts.
    fn list_prompts(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Prompt>, McpError>> + Send;

    /// Get a prompt with the given arguments.
    fn get_prompt(
        &self,
        name: &str,
        args: Option<serde_json::Map<String, Value>>,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<GetPromptResult, McpError>> + Send;
}

/// Handler for task-related operations.
///
/// Implement this trait to support long-running operations that can be
/// tracked, monitored, and cancelled.
pub trait TaskHandler: Send + Sync {
    /// List all tasks, optionally filtered by status.
    fn list_tasks(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Task>, McpError>> + Send;

    /// Get the current state of a task.
    fn get_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Option<Task>, McpError>> + Send;

    /// Cancel a running task.
    fn cancel_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send;
}

/// Handler for completion suggestions.
///
/// Implement this trait to provide autocomplete suggestions for
/// resource URIs, prompt arguments, etc.
pub trait CompletionHandler: Send + Sync {
    /// Complete a partial resource URI.
    fn complete_resource(
        &self,
        partial_uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<String>, McpError>> + Send;

    /// Complete a partial prompt argument.
    fn complete_prompt_arg(
        &self,
        prompt_name: &str,
        arg_name: &str,
        partial_value: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<String>, McpError>> + Send;
}

/// Handler for logging operations.
///
/// Implement this trait to handle log messages from the client.
pub trait LoggingHandler: Send + Sync {
    /// Set the current logging level.
    fn set_level(&self, level: LogLevel) -> impl Future<Output = Result<(), McpError>> + Send;
}

/// Log levels for MCP logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LogLevel {
    /// Debug level - most verbose.
    Debug,
    /// Info level.
    #[default]
    Info,
    /// Notice level.
    Notice,
    /// Warning level.
    Warning,
    /// Error level.
    Error,
    /// Critical level.
    Critical,
    /// Alert level.
    Alert,
    /// Emergency level - most severe.
    Emergency,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Info => write!(f, "info"),
            Self::Notice => write!(f, "notice"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
            Self::Critical => write!(f, "critical"),
            Self::Alert => write!(f, "alert"),
            Self::Emergency => write!(f, "emergency"),
        }
    }
}

/// Handler for sampling requests (server-initiated LLM calls).
///
/// Implement this trait to allow the server to request LLM completions
/// from the client. This enables agentic workflows where servers
/// can leverage the client's AI capabilities.
///
/// # Example
///
/// ```ignore
/// use mcpkit_server::{SamplingHandler, Context};
/// use mcpkit_core::types::sampling::{CreateMessageRequest, CreateMessageResult};
/// use mcpkit_core::error::McpError;
///
/// struct MyServer;
///
/// impl SamplingHandler for MyServer {
///     async fn create_message(
///         &self,
///         request: CreateMessageRequest,
///         ctx: &Context<'_>,
///     ) -> Result<CreateMessageResult, McpError> {
///         // The client will handle this request and invoke the LLM
///         ctx.peer().create_message(request).await
///     }
/// }
/// ```
///
/// # Note
///
/// Sampling is a client-side capability. The server sends a sampling request
/// to the client, which then invokes the LLM and returns the result. The
/// handler implementation typically just forwards the request through the
/// peer interface.
pub trait SamplingHandler: Send + Sync {
    /// Request the client to create an LLM message.
    ///
    /// This allows the server to leverage the client's AI capabilities
    /// for generating completions, answering questions, or performing
    /// other LLM-based operations.
    ///
    /// # Arguments
    ///
    /// * `request` - The sampling request specifying messages, model preferences, etc.
    /// * `ctx` - The request context providing access to the peer connection.
    ///
    /// # Returns
    ///
    /// The result of the LLM completion, including the generated content
    /// and metadata about the completion.
    fn create_message(
        &self,
        request: CreateMessageRequest,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<CreateMessageResult, McpError>> + Send;
}

/// Handler for elicitation requests (structured user input).
///
/// Implement this trait to allow the server to request structured input
/// from the user through the client. This enables interactive workflows
/// where servers can gather user preferences, confirmations, or data.
///
/// # Example
///
/// ```ignore
/// use mcpkit_server::{ElicitationHandler, Context};
/// use mcpkit_core::types::elicitation::{ElicitRequest, ElicitResult};
/// use mcpkit_core::error::McpError;
///
/// struct MyServer;
///
/// impl ElicitationHandler for MyServer {
///     async fn elicit(
///         &self,
///         request: ElicitRequest,
///         ctx: &Context<'_>,
///     ) -> Result<ElicitResult, McpError> {
///         // The client will display the request to the user
///         ctx.peer().elicit(request).await
///     }
/// }
/// ```
///
/// # Use Cases
///
/// - Requesting user confirmation before destructive operations
/// - Gathering user preferences or configuration
/// - Prompting for credentials or sensitive information
/// - Interactive wizards or multi-step forms
pub trait ElicitationHandler: Send + Sync {
    /// Request structured input from the user.
    ///
    /// This allows the server to gather information from the user through
    /// the client interface. The client will present the request to the
    /// user according to the specified schema and return their response.
    ///
    /// # Arguments
    ///
    /// * `request` - The elicitation request specifying the message and schema.
    /// * `ctx` - The request context providing access to the peer connection.
    ///
    /// # Returns
    ///
    /// The result of the elicitation, including the user's action (accept,
    /// decline, or cancel) and any provided content.
    fn elicit(
        &self,
        request: ElicitRequest,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<ElicitResult, McpError>> + Send;
}

// =============================================================================
// Blanket implementations for Arc<T>
//
// These allow sharing a single handler instance across multiple registrations
// without requiring Clone on the user's type. The macro-generated `into_server()`
// method uses Arc internally to wire everything up automatically.
// =============================================================================

use std::sync::Arc;

impl<T: ServerHandler> ServerHandler for Arc<T> {
    fn server_info(&self) -> ServerInfo {
        (**self).server_info()
    }

    fn capabilities(&self) -> ServerCapabilities {
        (**self).capabilities()
    }

    fn instructions(&self) -> Option<String> {
        (**self).instructions()
    }

    fn on_initialized(&self, ctx: &Context<'_>) -> impl Future<Output = ()> + Send {
        (**self).on_initialized(ctx)
    }

    fn on_shutdown(&self) -> impl Future<Output = ()> + Send {
        (**self).on_shutdown()
    }
}

impl<T: ToolHandler> ToolHandler for Arc<T> {
    fn list_tools(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Tool>, McpError>> + Send {
        (**self).list_tools(ctx)
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<ToolOutput, McpError>> + Send {
        (**self).call_tool(name, args, ctx)
    }

    fn on_tools_changed(&self) -> impl Future<Output = ()> + Send {
        (**self).on_tools_changed()
    }
}

impl<T: ResourceHandler> ResourceHandler for Arc<T> {
    fn list_resources(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Resource>, McpError>> + Send {
        (**self).list_resources(ctx)
    }

    fn list_resource_templates(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<ResourceTemplate>, McpError>> + Send {
        (**self).list_resource_templates(ctx)
    }

    fn read_resource(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<ResourceContents>, McpError>> + Send {
        (**self).read_resource(uri, ctx)
    }

    fn subscribe(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        (**self).subscribe(uri, ctx)
    }

    fn unsubscribe(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        (**self).unsubscribe(uri, ctx)
    }
}

impl<T: PromptHandler> PromptHandler for Arc<T> {
    fn list_prompts(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Prompt>, McpError>> + Send {
        (**self).list_prompts(ctx)
    }

    fn get_prompt(
        &self,
        name: &str,
        args: Option<serde_json::Map<String, Value>>,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<GetPromptResult, McpError>> + Send {
        (**self).get_prompt(name, args, ctx)
    }
}

impl<T: TaskHandler> TaskHandler for Arc<T> {
    fn list_tasks(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Task>, McpError>> + Send {
        (**self).list_tasks(ctx)
    }

    fn get_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Option<Task>, McpError>> + Send {
        (**self).get_task(id, ctx)
    }

    fn cancel_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        (**self).cancel_task(id, ctx)
    }
}

impl<T: CompletionHandler> CompletionHandler for Arc<T> {
    fn complete_resource(
        &self,
        partial_uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<String>, McpError>> + Send {
        (**self).complete_resource(partial_uri, ctx)
    }

    fn complete_prompt_arg(
        &self,
        prompt_name: &str,
        arg_name: &str,
        partial_value: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<String>, McpError>> + Send {
        (**self).complete_prompt_arg(prompt_name, arg_name, partial_value, ctx)
    }
}

impl<T: LoggingHandler> LoggingHandler for Arc<T> {
    fn set_level(&self, level: LogLevel) -> impl Future<Output = Result<(), McpError>> + Send {
        (**self).set_level(level)
    }
}

impl<T: SamplingHandler> SamplingHandler for Arc<T> {
    fn create_message(
        &self,
        request: CreateMessageRequest,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<CreateMessageResult, McpError>> + Send {
        (**self).create_message(request, ctx)
    }
}

impl<T: ElicitationHandler> ElicitationHandler for Arc<T> {
    fn elicit(
        &self,
        request: ElicitRequest,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<ElicitResult, McpError>> + Send {
        (**self).elicit(request, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestServer;

    impl ServerHandler for TestServer {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("test", "1.0.0")
        }
    }

    #[test]
    fn test_server_handler() {
        let server = TestServer;
        let info = server.server_info();
        assert_eq!(info.name, "test");
        assert_eq!(info.version, "1.0.0");
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Error);
        assert!(LogLevel::Info < LogLevel::Warning);
        assert!(LogLevel::Emergency > LogLevel::Alert);
    }

    #[test]
    fn test_arc_server_handler() {
        let server = Arc::new(TestServer);
        let info = server.server_info();
        assert_eq!(info.name, "test");
        assert_eq!(info.version, "1.0.0");
    }
}
