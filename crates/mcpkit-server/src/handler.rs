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
    CancelTaskResult, CompleteRequest, CompleteResult, GetPromptResult, GetTaskResult,
    ListTasksResult, Prompt, Resource, ResourceContents, ResourceTemplate, TaskId, Tool,
    ToolOutput,
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

    /// Called when the client sends `notifications/roots/list_changed`.
    ///
    /// Only invoked when the client advertised the `roots` capability. A good
    /// place to invalidate cached roots and re-request them via
    /// [`Context::list_roots`]. The default is a no-op.
    fn on_roots_list_changed(&self, _ctx: &Context<'_>) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when the connection is about to be closed.
    fn on_shutdown(&self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Handle a `logging/setLevel` request: the client's requested minimum log
    /// severity. Only reached when the server advertises the `logging`
    /// capability. The default is a no-op.
    fn set_log_level(
        &self,
        _level: LogLevel,
        _ctx: &Context<'_>,
    ) -> impl Future<Output = Result<(), McpError>> + Send {
        async { Ok(()) }
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
    /// `args` is passed through **unvalidated**: this generic path does not check
    /// `args` against the tool's `inputSchema`, nor the returned
    /// `structuredContent` against its `outputSchema`. Enable the
    /// `schema-validation` feature and wrap the handler with
    /// [`ServerBuilder::validate_tool_io`](crate::builder::ServerBuilder::validate_tool_io)
    /// (or [`ValidatingToolHandler`](crate::validation::ValidatingToolHandler) for
    /// adapter users) to enforce those schemas.
    ///
    /// # CPU-bound or blocking work
    ///
    /// The stdio runtime drives requests cooperatively on one task, so a
    /// `call_tool` that does heavy CPU work (or blocks) *before* awaiting stalls
    /// all other in-flight requests until it yields. Offload the hot section to
    /// your runtime's blocking/thread mechanism — `tokio::task::spawn_blocking`,
    /// `std::thread`, `rayon`, etc. — and `.await` its result. The same applies to
    /// task-augmented tools: background task futures are polled by the same
    /// cooperative loop.
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
    /// List all tasks.
    ///
    /// The [`ListTasksResult`] wrapper may carry `nextCursor` and result-level
    /// `_meta`; `Vec<Task>::into()` produces one with neither. (Request-cursor
    /// input for real pagination is not yet threaded — see #150.)
    fn list_tasks(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<ListTasksResult, McpError>> + Send;

    /// Get the current state of a task.
    ///
    /// Return `Ok(None)` for an unknown task. The [`GetTaskResult`] wrapper may
    /// carry result-level `_meta`; `Task::into()` produces one with no metadata.
    fn get_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Option<GetTaskResult>, McpError>> + Send;

    /// Cancel a running task, returning its post-cancellation state.
    ///
    /// - `Ok(Some(result))` — cancellation accepted; `result` is the task's state
    ///   afterwards (and may carry result-level `_meta`).
    /// - `Ok(None)` — no such task.
    /// - `Err(..)` — the task exists but cancellation failed (an internal error).
    fn cancel_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Option<CancelTaskResult>, McpError>> + Send;
}

/// Handler for completion suggestions (`completion/complete`).
///
/// Implement this trait to provide autocomplete suggestions for prompt
/// arguments and resource-template variables. The full [`CompleteRequest`] is
/// passed so a handler can dispatch on the [`ref`](CompleteRequest::ref_),
/// read the [`argument`](CompleteRequest::argument) being typed, and use any
/// previously-resolved [`context`](CompleteRequest::context). Return a
/// [`CompleteResult`] — build one from a
/// [`Completion`](mcpkit_core::types::Completion) via `.into()` for the common
/// case, or attach result-level `_meta` with [`CompleteResult::with_meta`]. The
/// route layer caps `values` at
/// [`MAX_COMPLETION_VALUES`](mcpkit_core::types::MAX_COMPLETION_VALUES).
pub trait CompletionHandler: Send + Sync {
    /// Produce completion suggestions for the request.
    fn complete(
        &self,
        request: &CompleteRequest,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<CompleteResult, McpError>> + Send;
}

/// The MCP logging severity, re-exported from core.
///
/// Inbound `logging/setLevel` is handled by [`ServerHandler::set_log_level`];
/// outbound `notifications/message` is emitted via the server's `log` helpers.
pub use mcpkit_core::types::LoggingLevel as LogLevel;

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

    fn on_roots_list_changed(&self, ctx: &Context<'_>) -> impl Future<Output = ()> + Send {
        (**self).on_roots_list_changed(ctx)
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
    ) -> impl Future<Output = Result<ListTasksResult, McpError>> + Send {
        (**self).list_tasks(ctx)
    }

    fn get_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Option<GetTaskResult>, McpError>> + Send {
        (**self).get_task(id, ctx)
    }

    fn cancel_task(
        &self,
        id: &TaskId,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Option<CancelTaskResult>, McpError>> + Send {
        (**self).cancel_task(id, ctx)
    }
}

impl<T: CompletionHandler> CompletionHandler for Arc<T> {
    fn complete(
        &self,
        request: &CompleteRequest,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<CompleteResult, McpError>> + Send {
        (**self).complete(request, ctx)
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
