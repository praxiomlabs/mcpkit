//! MCP client implementation.
//!
//! The [`Client`] struct provides a high-level API for interacting with
//! MCP servers. It handles:
//!
//! - Protocol initialization
//! - Request/response correlation
//! - Tool, resource, and prompt operations
//! - Task tracking
//! - Connection lifecycle

use futures::channel::oneshot;
use mcp_core::capability::{
    ClientCapabilities, ClientInfo, InitializeRequest, InitializeResult, ServerCapabilities,
    ServerInfo, PROTOCOL_VERSION,
};
use mcp_core::error::{HandshakeDetails, McpError, TransportContext, TransportDetails, TransportErrorKind};
use mcp_core::protocol::{Message, Notification, Request, RequestId, Response};
use mcp_core::types::{
    CallToolRequest, CallToolResult, GetPromptRequest, GetPromptResult, ListPromptsResult,
    ListResourcesResult, ListResourceTemplatesResult, ListToolsResult, Prompt, ReadResourceRequest,
    ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Tool,
};
use mcp_transport::Transport;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, trace, warn};

/// An MCP client connected to a server.
///
/// The client provides methods for interacting with MCP servers:
///
/// - Tools: `list_tools()`, `call_tool()`
/// - Resources: `list_resources()`, `read_resource()`
/// - Prompts: `list_prompts()`, `get_prompt()`
/// - Tasks: `list_tasks()`, `get_task()`, `cancel_task()`
///
/// # Example
///
/// ```ignore
/// let client = ClientBuilder::new()
///     .name("my-client")
///     .version("1.0.0")
///     .build(transport)
///     .await?;
///
/// let tools = client.list_tools().await?;
/// ```
pub struct Client<T: Transport> {
    /// The underlying transport.
    transport: Arc<T>,
    /// Server information received during initialization.
    server_info: ServerInfo,
    /// Server capabilities.
    server_caps: ServerCapabilities,
    /// Client information.
    client_info: ClientInfo,
    /// Client capabilities.
    client_caps: ClientCapabilities,
    /// Next request ID.
    next_id: AtomicU64,
    /// Pending requests awaiting responses.
    pending: Arc<RwLock<HashMap<RequestId, oneshot::Sender<Response>>>>,
    /// Instructions from the server.
    instructions: Option<String>,
}

impl<T: Transport> Client<T> {
    /// Create a new client (called by builder).
    pub(crate) fn new(
        transport: T,
        init_result: InitializeResult,
        client_info: ClientInfo,
        client_caps: ClientCapabilities,
    ) -> Self {
        Self {
            transport: Arc::new(transport),
            server_info: init_result.server_info,
            server_caps: init_result.capabilities,
            client_info,
            client_caps,
            next_id: AtomicU64::new(1),
            pending: Arc::new(RwLock::new(HashMap::new())),
            instructions: init_result.instructions,
        }
    }

    /// Get the server information.
    pub fn server_info(&self) -> &ServerInfo {
        &self.server_info
    }

    /// Get the server capabilities.
    pub fn server_capabilities(&self) -> &ServerCapabilities {
        &self.server_caps
    }

    /// Get the client information.
    pub fn client_info(&self) -> &ClientInfo {
        &self.client_info
    }

    /// Get the client capabilities.
    pub fn client_capabilities(&self) -> &ClientCapabilities {
        &self.client_caps
    }

    /// Get the server instructions, if provided.
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Check if the server supports tools.
    pub fn has_tools(&self) -> bool {
        self.server_caps.has_tools()
    }

    /// Check if the server supports resources.
    pub fn has_resources(&self) -> bool {
        self.server_caps.has_resources()
    }

    /// Check if the server supports prompts.
    pub fn has_prompts(&self) -> bool {
        self.server_caps.has_prompts()
    }

    /// Check if the server supports tasks.
    pub fn has_tasks(&self) -> bool {
        self.server_caps.has_tasks()
    }

    // ==========================================================================
    // Tool Operations
    // ==========================================================================

    /// List all available tools.
    ///
    /// # Errors
    ///
    /// Returns an error if tools are not supported or the request fails.
    pub async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        self.ensure_capability("tools", self.has_tools())?;

        let result: ListToolsResult = self.request("tools/list", None).await?;
        Ok(result.tools)
    }

    /// List tools with pagination.
    ///
    /// # Errors
    ///
    /// Returns an error if tools are not supported or the request fails.
    pub async fn list_tools_paginated(
        &self,
        cursor: Option<&str>,
    ) -> Result<ListToolsResult, McpError> {
        self.ensure_capability("tools", self.has_tools())?;

        let params = cursor.map(|c| serde_json::json!({ "cursor": c }));
        self.request("tools/list", params).await
    }

    /// Call a tool by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to call
    /// * `arguments` - The arguments to pass to the tool (as JSON)
    ///
    /// # Errors
    ///
    /// Returns an error if tools are not supported or the call fails.
    pub async fn call_tool(
        &self,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_capability("tools", self.has_tools())?;

        let request = CallToolRequest {
            name: name.into(),
            arguments: Some(arguments),
        };
        self.request("tools/call", Some(serde_json::to_value(request)?))
            .await
    }

    // ==========================================================================
    // Resource Operations
    // ==========================================================================

    /// List all available resources.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are not supported or the request fails.
    pub async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        let result: ListResourcesResult = self.request("resources/list", None).await?;
        Ok(result.resources)
    }

    /// List resources with pagination.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are not supported or the request fails.
    pub async fn list_resources_paginated(
        &self,
        cursor: Option<&str>,
    ) -> Result<ListResourcesResult, McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        let params = cursor.map(|c| serde_json::json!({ "cursor": c }));
        self.request("resources/list", params).await
    }

    /// List resource templates.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are not supported or the request fails.
    pub async fn list_resource_templates(&self) -> Result<Vec<ResourceTemplate>, McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        let result: ListResourceTemplatesResult =
            self.request("resources/templates/list", None).await?;
        Ok(result.resource_templates)
    }

    /// Read a resource by URI.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are not supported or the read fails.
    pub async fn read_resource(&self, uri: impl Into<String>) -> Result<Vec<ResourceContents>, McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        let request = ReadResourceRequest { uri: uri.into() };
        let result: ReadResourceResult =
            self.request("resources/read", Some(serde_json::to_value(request)?))
                .await?;
        Ok(result.contents)
    }

    // ==========================================================================
    // Prompt Operations
    // ==========================================================================

    /// List all available prompts.
    ///
    /// # Errors
    ///
    /// Returns an error if prompts are not supported or the request fails.
    pub async fn list_prompts(&self) -> Result<Vec<Prompt>, McpError> {
        self.ensure_capability("prompts", self.has_prompts())?;

        let result: ListPromptsResult = self.request("prompts/list", None).await?;
        Ok(result.prompts)
    }

    /// List prompts with pagination.
    ///
    /// # Errors
    ///
    /// Returns an error if prompts are not supported or the request fails.
    pub async fn list_prompts_paginated(
        &self,
        cursor: Option<&str>,
    ) -> Result<ListPromptsResult, McpError> {
        self.ensure_capability("prompts", self.has_prompts())?;

        let params = cursor.map(|c| serde_json::json!({ "cursor": c }));
        self.request("prompts/list", params).await
    }

    /// Get a prompt by name, optionally with arguments.
    ///
    /// # Errors
    ///
    /// Returns an error if prompts are not supported or the get fails.
    pub async fn get_prompt(
        &self,
        name: impl Into<String>,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<GetPromptResult, McpError> {
        self.ensure_capability("prompts", self.has_prompts())?;

        let request = GetPromptRequest {
            name: name.into(),
            arguments,
        };
        self.request("prompts/get", Some(serde_json::to_value(request)?))
            .await
    }

    // ==========================================================================
    // Connection Operations
    // ==========================================================================

    /// Ping the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the ping fails.
    pub async fn ping(&self) -> Result<(), McpError> {
        let _: serde_json::Value = self.request("ping", None).await?;
        Ok(())
    }

    /// Close the connection gracefully.
    ///
    /// # Errors
    ///
    /// Returns an error if the close fails.
    pub async fn close(self) -> Result<(), McpError> {
        debug!("Closing client connection");
        self.transport.close().await.map_err(|e| {
            McpError::Transport(Box::new(TransportDetails {
                kind: TransportErrorKind::ConnectionClosed,
                message: e.to_string(),
                context: TransportContext::default(),
                source: None,
            }))
        })
    }

    // ==========================================================================
    // Internal Methods
    // ==========================================================================

    /// Generate the next request ID.
    fn next_request_id(&self) -> RequestId {
        RequestId::Number(self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// Send a request and wait for the response.
    async fn request<R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<R, McpError> {
        let id = self.next_request_id();
        let request = if let Some(params) = params {
            Request::with_params(method.to_string(), id.clone(), params)
        } else {
            Request::new(method.to_string(), id.clone())
        };

        trace!(?id, method, "Sending request");

        // Create a channel for the response
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.write().await;
            pending.insert(id.clone(), tx);
        }

        // Send the request
        self.transport
            .send(Message::Request(request))
            .await
            .map_err(|e| McpError::Transport(Box::new(TransportDetails {
                kind: TransportErrorKind::WriteFailed,
                message: e.to_string(),
                context: TransportContext::default(),
                source: None,
            })))?;

        // Wait for the response
        let response = self.wait_for_response(id.clone(), rx).await?;

        // Process the response
        if let Some(error) = response.error {
            return Err(McpError::Internal {
                message: error.message,
                source: None,
            });
        }

        let result = response.result.ok_or_else(|| McpError::Internal {
            message: "Response contained neither result nor error".to_string(),
            source: None,
        })?;

        serde_json::from_value(result).map_err(McpError::from)
    }

    /// Wait for a response to a request.
    async fn wait_for_response(
        &self,
        id: RequestId,
        _rx: oneshot::Receiver<Response>,
    ) -> Result<Response, McpError> {
        // In a real implementation, we would have a background task reading messages
        // and routing responses. For now, we do a simple blocking receive with a loop.

        // Read messages until we get our response
        loop {
            match self.transport.recv().await {
                Ok(Some(Message::Response(response))) => {
                    if response.id == id {
                        return Ok(response);
                    }
                    // Route to correct pending request
                    let mut pending = self.pending.write().await;
                    if let Some(sender) = pending.remove(&response.id) {
                        let _ = sender.send(response);
                    } else {
                        warn!(?response.id, "Received response for unknown request");
                    }
                }
                Ok(Some(Message::Notification(notification))) => {
                    trace!(method = %notification.method, "Received notification");
                    // Handle notifications (could emit events)
                }
                Ok(Some(Message::Request(request))) => {
                    trace!(method = %request.method, "Received server request");
                    // Handle server-initiated requests
                    // In a full implementation, this would delegate to ClientHandler
                }
                Ok(None) => {
                    error!("Connection closed while waiting for response");
                    return Err(McpError::Transport(Box::new(TransportDetails {
                        kind: TransportErrorKind::ConnectionClosed,
                        message: "Connection closed".to_string(),
                        context: TransportContext::default(),
                        source: None,
                    })));
                }
                Err(e) => {
                    error!(?e, "Transport error while waiting for response");
                    return Err(McpError::Transport(Box::new(TransportDetails {
                        kind: TransportErrorKind::ReadFailed,
                        message: e.to_string(),
                        context: TransportContext::default(),
                        source: None,
                    })));
                }
            }
        }
    }

    /// Check that a capability is supported.
    fn ensure_capability(&self, name: &str, supported: bool) -> Result<(), McpError> {
        if supported {
            Ok(())
        } else {
            Err(McpError::CapabilityNotSupported {
                capability: name.to_string(),
                available: self.available_capabilities().into_boxed_slice(),
            })
        }
    }

    /// Get list of available capabilities.
    fn available_capabilities(&self) -> Vec<String> {
        let mut caps = Vec::new();
        if self.has_tools() {
            caps.push("tools".to_string());
        }
        if self.has_resources() {
            caps.push("resources".to_string());
        }
        if self.has_prompts() {
            caps.push("prompts".to_string());
        }
        if self.has_tasks() {
            caps.push("tasks".to_string());
        }
        caps
    }
}

/// Initialize a client connection.
///
/// This performs the MCP handshake:
/// 1. Send initialize request
/// 2. Wait for initialize result
/// 3. Send initialized notification
pub(crate) async fn initialize<T: Transport>(
    transport: &T,
    client_info: &ClientInfo,
    capabilities: &ClientCapabilities,
) -> Result<InitializeResult, McpError> {
    debug!("Initializing MCP connection");

    // Build initialize request
    let request = InitializeRequest::new(client_info.clone(), capabilities.clone());
    let init_request = Request::with_params(
        "initialize".to_string(),
        RequestId::Number(0),
        serde_json::to_value(&request)?,
    );

    // Send initialize request
    transport
        .send(Message::Request(init_request))
        .await
        .map_err(|e| McpError::Transport(Box::new(TransportDetails {
            kind: TransportErrorKind::WriteFailed,
            message: format!("Failed to send initialize: {e}"),
            context: TransportContext::default(),
            source: None,
        })))?;

    // Wait for response
    let response = loop {
        match transport.recv().await {
            Ok(Some(Message::Response(r))) if r.id == RequestId::Number(0) => break r,
            Ok(Some(_)) => continue,
            Ok(None) => {
                return Err(McpError::HandshakeFailed(Box::new(HandshakeDetails {
                    message: "Connection closed during initialization".to_string(),
                    client_version: Some(PROTOCOL_VERSION.to_string()),
                    server_version: None,
                    source: None,
                })));
            }
            Err(e) => {
                return Err(McpError::HandshakeFailed(Box::new(HandshakeDetails {
                    message: format!("Transport error during initialization: {e}"),
                    client_version: Some(PROTOCOL_VERSION.to_string()),
                    server_version: None,
                    source: None,
                })));
            }
        }
    };

    // Parse the response
    if let Some(error) = response.error {
        return Err(McpError::HandshakeFailed(Box::new(HandshakeDetails {
            message: error.message,
            client_version: Some(PROTOCOL_VERSION.to_string()),
            server_version: None,
            source: None,
        })));
    }

    let result: InitializeResult = response
        .result
        .map(serde_json::from_value)
        .transpose()?
        .ok_or_else(|| McpError::HandshakeFailed(Box::new(HandshakeDetails {
            message: "Empty initialize result".to_string(),
            client_version: Some(PROTOCOL_VERSION.to_string()),
            server_version: None,
            source: None,
        })))?;

    debug!(
        server = %result.server_info.name,
        version = %result.server_info.version,
        "Received initialize result"
    );

    // Send initialized notification
    let notification = Notification::new("notifications/initialized");
    transport
        .send(Message::Notification(notification))
        .await
        .map_err(|e| McpError::Transport(Box::new(TransportDetails {
            kind: TransportErrorKind::WriteFailed,
            message: format!("Failed to send initialized: {e}"),
            context: TransportContext::default(),
            source: None,
        })))?;

    debug!("MCP initialization complete");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_generation() {
        let next_id = AtomicU64::new(1);
        assert_eq!(next_id.fetch_add(1, Ordering::SeqCst), 1);
        assert_eq!(next_id.fetch_add(1, Ordering::SeqCst), 2);
    }
}
