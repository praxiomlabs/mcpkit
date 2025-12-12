//! Client handler traits for server-initiated requests.
//!
//! MCP servers can initiate certain requests to clients, such as:
//!
//! - **Sampling**: Request the client's LLM to generate a response
//! - **Elicitation**: Request user input through the client
//! - **Roots**: Get file system roots that the client exposes
//!
//! This module defines traits that clients can implement to handle these requests.

use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    CreateMessageRequest, CreateMessageResult, ElicitRequest, ElicitResult,
    TaskProgress, TaskId,
};
use std::future::Future;

/// Handler trait for server-initiated requests.
///
/// Implement this trait to handle requests that servers send to clients.
/// All methods have default implementations that return "not supported" errors.
///
/// # Example
///
/// ```rust
/// use mcpkit_client::ClientHandler;
/// use mcpkit_core::types::{CreateMessageRequest, CreateMessageResult};
/// use mcpkit_core::error::McpError;
///
/// struct MyHandler;
///
/// impl ClientHandler for MyHandler {
///     // Override methods as needed to handle server requests
/// }
/// ```
pub trait ClientHandler: Send + Sync {
    /// Handle a sampling request from the server.
    ///
    /// The server is asking the client's LLM to generate a response.
    /// This is used for agentic workflows where the server needs LLM capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error if sampling is not supported or the request fails.
    fn create_message(
        &self,
        _request: CreateMessageRequest,
    ) -> impl Future<Output = Result<CreateMessageResult, McpError>> + Send {
        async {
            Err(McpError::CapabilityNotSupported {
                capability: "sampling".to_string(),
                available: Box::new([]),
            })
        }
    }

    /// Handle an elicitation request from the server.
    ///
    /// The server is asking for user input. The client should present
    /// the request to the user and return their response.
    ///
    /// # Errors
    ///
    /// Returns an error if elicitation is not supported or the request fails.
    fn elicit(
        &self,
        _request: ElicitRequest,
    ) -> impl Future<Output = Result<ElicitResult, McpError>> + Send {
        async {
            Err(McpError::CapabilityNotSupported {
                capability: "elicitation".to_string(),
                available: Box::new([]),
            })
        }
    }

    /// List roots that the client exposes.
    ///
    /// Roots are file system paths that the server can access.
    /// This is typically used for file-based operations.
    ///
    /// # Errors
    ///
    /// Returns an error if roots are not supported.
    fn list_roots(&self) -> impl Future<Output = Result<Vec<Root>, McpError>> + Send {
        async {
            Err(McpError::CapabilityNotSupported {
                capability: "roots".to_string(),
                available: Box::new([]),
            })
        }
    }

    /// Called when the connection is established.
    ///
    /// Override this to perform setup after initialization.
    fn on_connected(&self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when the connection is closed.
    ///
    /// Override this to perform cleanup.
    fn on_disconnected(&self) -> impl Future<Output = ()> + Send {
        async {}
    }

    // =========================================================================
    // Notification Handlers
    // =========================================================================

    /// Called when a task makes progress.
    ///
    /// Override this to track task progress updates from the server.
    fn on_task_progress(
        &self,
        _task_id: TaskId,
        _progress: TaskProgress,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when a resource has been updated.
    ///
    /// Override this to react to resource changes (requires subscription).
    fn on_resource_updated(&self, _uri: String) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when the list of available resources has changed.
    ///
    /// Override this to refresh your cached resource list.
    fn on_resources_list_changed(&self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when the list of available tools has changed.
    ///
    /// Override this to refresh your cached tool list.
    fn on_tools_list_changed(&self) -> impl Future<Output = ()> + Send {
        async {}
    }

    /// Called when the list of available prompts has changed.
    ///
    /// Override this to refresh your cached prompt list.
    fn on_prompts_list_changed(&self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// A root directory that the client exposes to servers.
#[derive(Debug, Clone)]
pub struct Root {
    /// URI of the root (e.g., "file:///home/user/project").
    pub uri: String,
    /// Human-readable name for the root.
    pub name: Option<String>,
}

impl Root {
    /// Create a new root.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: None,
        }
    }

    /// Set the name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// A no-op handler that rejects all server requests.
///
/// Use this as a default handler when you don't need to handle
/// any server-initiated requests.
pub struct NoOpHandler;

impl ClientHandler for NoOpHandler {}

/// A handler that supports sampling by delegating to a closure.
pub struct SamplingHandler<F> {
    handler: F,
}

impl<F, Fut> SamplingHandler<F>
where
    F: Fn(CreateMessageRequest) -> Fut + Send + Sync,
    Fut: Future<Output = Result<CreateMessageResult, McpError>> + Send,
{
    /// Create a new sampling handler.
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

impl<F, Fut> ClientHandler for SamplingHandler<F>
where
    F: Fn(CreateMessageRequest) -> Fut + Send + Sync,
    Fut: Future<Output = Result<CreateMessageResult, McpError>> + Send,
{
    fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> impl Future<Output = Result<CreateMessageResult, McpError>> + Send {
        (self.handler)(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_builder() {
        let root = Root::new("file:///home/user/project").name("My Project");
        assert!(root.uri.contains("project"));
        assert_eq!(root.name, Some("My Project".to_string()));
    }

    #[tokio::test]
    async fn test_noop_handler() {
        let handler = NoOpHandler;
        let result = handler
            .create_message(CreateMessageRequest {
                messages: vec![],
                model_preferences: None,
                system_prompt: None,
                include_context: None,
                temperature: None,
                max_tokens: 100,
                stop_sequences: None,
                metadata: None,
            })
            .await;
        assert!(result.is_err());
    }
}
