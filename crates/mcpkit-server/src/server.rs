//! Server runtime for MCP servers.
//!
//! This module provides the runtime that executes an MCP server over
//! a transport, handling message routing, request correlation, and
//! the connection lifecycle.
//!
//! # Overview
//!
//! The server runtime:
//! 1. Accepts a transport for communication
//! 2. Handles the initialize/initialized handshake
//! 3. Routes incoming requests to the appropriate handlers
//! 4. Manages the connection lifecycle
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::{ServerBuilder, ServerHandler, ServerState};
//! use mcpkit_core::capability::{ServerInfo, ServerCapabilities};
//!
//! struct MyHandler;
//! impl ServerHandler for MyHandler {
//!     fn server_info(&self) -> ServerInfo {
//!         ServerInfo::new("my-server", "1.0.0")
//!     }
//! }
//!
//! // Build a server and create server state
//! let server = ServerBuilder::new(MyHandler).build();
//! let state = ServerState::new(server.capabilities().clone());
//!
//! assert!(!state.is_initialized());
//! ```

use crate::builder::{NotRegistered, Registered, Server};
use crate::context::{CancellationToken, Context, Peer};
use crate::handler::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use mcpkit_core::capability::{
    negotiate_version, ClientCapabilities, ServerCapabilities, SUPPORTED_PROTOCOL_VERSIONS,
};
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::{Message, Notification, ProgressToken, Request, Response};
use mcpkit_core::types::CallToolResult;
use mcpkit_transport::Transport;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

/// State for a running server.
pub struct ServerState {
    /// Client capabilities negotiated during initialization.
    pub client_caps: RwLock<ClientCapabilities>,
    /// Server capabilities advertised during initialization.
    pub server_caps: ServerCapabilities,
    /// Whether the server has been initialized.
    pub initialized: AtomicBool,
    /// Active cancellation tokens by request ID.
    pub cancellations: RwLock<HashMap<String, CancellationToken>>,
    /// The protocol version negotiated during initialization.
    pub negotiated_version: RwLock<Option<String>>,
}

impl ServerState {
    /// Create a new server state.
    pub fn new(server_caps: ServerCapabilities) -> Self {
        Self {
            client_caps: RwLock::new(ClientCapabilities::default()),
            server_caps,
            initialized: AtomicBool::new(false),
            cancellations: RwLock::new(HashMap::new()),
            negotiated_version: RwLock::new(None),
        }
    }

    /// Get the negotiated protocol version.
    ///
    /// Returns `None` if not yet initialized.
    pub fn protocol_version(&self) -> Option<String> {
        self.negotiated_version
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Set the negotiated protocol version.
    ///
    /// Silently fails if the lock is poisoned.
    pub fn set_protocol_version(&self, version: String) {
        if let Ok(mut guard) = self.negotiated_version.write() {
            *guard = Some(version);
        }
    }

    /// Get a snapshot of client capabilities.
    ///
    /// Returns default capabilities if the lock is poisoned.
    pub fn client_caps(&self) -> ClientCapabilities {
        self.client_caps
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Update client capabilities.
    ///
    /// Silently fails if the lock is poisoned.
    pub fn set_client_caps(&self, caps: ClientCapabilities) {
        if let Ok(mut guard) = self.client_caps.write() {
            *guard = caps;
        }
    }

    /// Check if the server is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Mark the server as initialized.
    pub fn set_initialized(&self) {
        self.initialized.store(true, Ordering::Release);
    }

    /// Register a cancellation token for a request.
    pub fn register_cancellation(&self, request_id: &str, token: CancellationToken) {
        if let Ok(mut cancellations) = self.cancellations.write() {
            cancellations.insert(request_id.to_string(), token);
        }
    }

    /// Cancel a request by ID.
    pub fn cancel_request(&self, request_id: &str) {
        if let Ok(cancellations) = self.cancellations.read() {
            if let Some(token) = cancellations.get(request_id) {
                token.cancel();
            }
        }
    }

    /// Remove a cancellation token after request completion.
    pub fn remove_cancellation(&self, request_id: &str) {
        if let Ok(mut cancellations) = self.cancellations.write() {
            cancellations.remove(request_id);
        }
    }
}

/// A peer implementation that sends notifications over a transport.
pub struct TransportPeer<T: Transport> {
    transport: Arc<T>,
}

impl<T: Transport> TransportPeer<T> {
    /// Create a new transport peer.
    pub fn new(transport: Arc<T>) -> Self {
        Self { transport }
    }
}

impl<T: Transport + 'static> Peer for TransportPeer<T>
where
    T::Error: Into<McpError>,
{
    fn notify(
        &self,
        notification: Notification,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), McpError>> + Send + '_>>
    {
        let transport = self.transport.clone();
        Box::pin(async move {
            transport
                .send(Message::Notification(notification))
                .await
                .map_err(|e| e.into())
        })
    }
}

/// Server runtime configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Whether to automatically send initialized notification.
    pub auto_initialized: bool,
    /// Maximum concurrent requests to process.
    pub max_concurrent_requests: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            auto_initialized: true,
            max_concurrent_requests: 100,
        }
    }
}

/// Server runtime that handles the message loop.
///
/// This runtime manages the connection lifecycle, routes requests to
/// handlers, and coordinates response delivery.
pub struct ServerRuntime<S, Tr>
where
    Tr: Transport,
{
    server: S,
    transport: Arc<Tr>,
    state: Arc<ServerState>,
    /// Runtime configuration (request timeouts, etc.) - will be used by advanced features.
    #[allow(dead_code)]
    config: RuntimeConfig,
}

impl<S, Tr> ServerRuntime<S, Tr>
where
    S: RequestRouter + Send + Sync,
    Tr: Transport + 'static,
    Tr::Error: Into<McpError>,
{
    /// Get the server state.
    pub fn state(&self) -> &Arc<ServerState> {
        &self.state
    }

    /// Run the server message loop.
    ///
    /// This method runs until the connection is closed or an error occurs.
    pub async fn run(&self) -> Result<(), McpError> {
        loop {
            match self.transport.recv().await {
                Ok(Some(message)) => {
                    if let Err(e) = self.handle_message(message).await {
                        tracing::error!(error = %e, "Error handling message");
                    }
                }
                Ok(None) => {
                    // Connection closed cleanly
                    tracing::info!("Connection closed");
                    break;
                }
                Err(e) => {
                    let err: McpError = e.into();
                    tracing::error!(error = %err, "Transport error");
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    /// Handle a single message.
    async fn handle_message(&self, message: Message) -> Result<(), McpError> {
        match message {
            Message::Request(request) => self.handle_request(request).await,
            Message::Notification(notification) => self.handle_notification(notification).await,
            Message::Response(_) => {
                // Servers don't typically receive responses
                tracing::warn!("Received unexpected response message");
                Ok(())
            }
        }
    }

    /// Handle a request.
    async fn handle_request(&self, request: Request) -> Result<(), McpError> {
        let method = request.method.to_string();
        let id = request.id.clone();

        tracing::debug!(method = %method, id = %id, "Handling request");

        let response = match method.as_str() {
            "initialize" => self.handle_initialize(&request).await,
            _ if !self.state.is_initialized() => {
                Err(McpError::invalid_request("Server not initialized"))
            }
            _ => self.route_request(&request).await,
        };

        // Send response
        let response_msg = match response {
            Ok(result) => Response::success(id, result),
            Err(e) => Response::error(id, e.into()),
        };

        self.transport
            .send(Message::Response(response_msg))
            .await
            .map_err(|e| e.into())
    }

    /// Handle the initialize request.
    ///
    /// This performs protocol version negotiation according to the MCP specification:
    /// 1. Client sends its preferred protocol version
    /// 2. Server responds with the same version if supported, or its preferred version
    /// 3. Client must support the returned version or disconnect
    async fn handle_initialize(
        &self,
        request: &Request,
    ) -> Result<serde_json::Value, McpError> {
        if self.state.is_initialized() {
            return Err(McpError::invalid_request("Already initialized"));
        }

        // Parse initialize params
        let params = request.params.as_ref().ok_or_else(|| {
            McpError::invalid_params("initialize", "missing params")
        })?;

        // Extract and negotiate protocol version
        let requested_version = params
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let negotiated_version = negotiate_version(requested_version);

        // Log version negotiation details for debugging
        if requested_version != negotiated_version {
            tracing::info!(
                requested = %requested_version,
                negotiated = %negotiated_version,
                supported = ?SUPPORTED_PROTOCOL_VERSIONS,
                "Protocol version negotiation: client requested unsupported version"
            );
        } else {
            tracing::debug!(
                version = %negotiated_version,
                "Protocol version negotiated successfully"
            );
        }

        // Store the negotiated version
        self.state.set_protocol_version(negotiated_version.to_string());

        // Extract client info and capabilities
        if let Some(caps) = params.get("capabilities") {
            if let Ok(client_caps) = serde_json::from_value::<ClientCapabilities>(caps.clone()) {
                self.state.set_client_caps(client_caps);
            }
        }

        // Build response with negotiated version
        let result = serde_json::json!({
            "protocolVersion": negotiated_version,
            "serverInfo": {
                "name": "mcp-server",
                "version": "1.0.0"
            },
            "capabilities": self.state.server_caps
        });

        self.state.set_initialized();

        Ok(result)
    }

    /// Route a request to the appropriate handler.
    async fn route_request(&self, request: &Request) -> Result<serde_json::Value, McpError> {
        let method = request.method.as_ref();
        let params = request.params.as_ref();

        // Extract progress token from params._meta.progressToken if present
        let progress_token = extract_progress_token(params);

        // Create context for the handler
        let peer = TransportPeer::new(self.transport.clone());
        let client_caps = self.state.client_caps();
        let ctx = Context::new(
            &request.id,
            progress_token.as_ref(),
            &client_caps,
            &self.state.server_caps,
            &peer,
        );

        // Delegate to the router
        self.server.route(method, params, &ctx).await
    }

    /// Handle a notification.
    async fn handle_notification(&self, notification: Notification) -> Result<(), McpError> {
        let method = notification.method.as_ref();

        tracing::debug!(method = %method, "Handling notification");

        match method {
            "notifications/initialized" => {
                tracing::info!("Client sent initialized notification");
                Ok(())
            }
            "notifications/cancelled" => {
                if let Some(params) = &notification.params {
                    if let Some(request_id) = params.get("requestId").and_then(|v| v.as_str()) {
                        self.state.cancel_request(request_id);
                    }
                }
                Ok(())
            }
            _ => {
                tracing::debug!(method = %method, "Ignoring unknown notification");
                Ok(())
            }
        }
    }
}

// Constructor implementations for ServerRuntime with different server types
impl<H, T, R, P, K, Tr> ServerRuntime<Server<H, T, R, P, K>, Tr>
where
    H: ServerHandler + Send + Sync,
    T: Send + Sync,
    R: Send + Sync,
    P: Send + Sync,
    K: Send + Sync,
    Tr: Transport + 'static,
    Tr::Error: Into<McpError>,
{
    /// Create a new server runtime.
    pub fn new(server: Server<H, T, R, P, K>, transport: Tr) -> Self {
        let caps = server.capabilities().clone();
        Self {
            server,
            transport: Arc::new(transport),
            state: Arc::new(ServerState::new(caps)),
            config: RuntimeConfig::default(),
        }
    }

    /// Create a new server runtime with custom configuration.
    pub fn with_config(server: Server<H, T, R, P, K>, transport: Tr, config: RuntimeConfig) -> Self {
        let caps = server.capabilities().clone();
        Self {
            server,
            transport: Arc::new(transport),
            state: Arc::new(ServerState::new(caps)),
            config,
        }
    }
}

/// Trait for routing requests to handlers.
///
/// This trait is implemented by Server with different bounds depending on
/// which handlers are registered.
#[allow(async_fn_in_trait)]
pub trait RequestRouter: Send + Sync {
    /// Route a request and return the result.
    async fn route(
        &self,
        method: &str,
        params: Option<&serde_json::Value>,
        ctx: &Context<'_>,
    ) -> Result<serde_json::Value, McpError>;
}

/// Extension methods for Server to run with a transport.
impl<H, T, R, P, K> Server<H, T, R, P, K>
where
    H: ServerHandler + Send + Sync + 'static,
    T: Send + Sync + 'static,
    R: Send + Sync + 'static,
    P: Send + Sync + 'static,
    K: Send + Sync + 'static,
    Self: RequestRouter,
{
    /// Run this server over the given transport.
    pub async fn serve<Tr>(self, transport: Tr) -> Result<(), McpError>
    where
        Tr: Transport + 'static,
        Tr::Error: Into<McpError>,
    {
        let runtime = ServerRuntime::new(self, transport);
        runtime.run().await
    }
}

// ============================================================================
// RequestRouter implementations via macro
// ============================================================================

// Internal routing functions to reduce code duplication.
// Each function handles a specific handler type's methods.

async fn route_tools<TH: ToolHandler + Send + Sync>(
    handler: &TH,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        "tools/list" => {
            let result = handler.list_tools(ctx).await;
            Some(result.map(|tools| serde_json::json!({ "tools": tools })))
        }
        "tools/call" => {
            let result = (|| async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params("tools/call", "missing params")
                })?;
                let name = params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("tools/call", "missing tool name"))?;
                let args = params.get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let output = handler.call_tool(name, args, ctx).await?;
                let result: CallToolResult = output.into();
                Ok(serde_json::to_value(result).unwrap_or(serde_json::json!({})))
            })().await;
            Some(result)
        }
        _ => None,
    }
}

async fn route_resources<RH: ResourceHandler + Send + Sync>(
    handler: &RH,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        "resources/list" => {
            let result = handler.list_resources(ctx).await;
            Some(result.map(|resources| serde_json::json!({ "resources": resources })))
        }
        "resources/read" => {
            let result = (|| async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params("resources/read", "missing params")
                })?;
                let uri = params.get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("resources/read", "missing uri"))?;
                let contents = handler.read_resource(uri, ctx).await?;
                Ok(serde_json::json!({ "contents": contents }))
            })().await;
            Some(result)
        }
        _ => None,
    }
}

async fn route_prompts<PH: PromptHandler + Send + Sync>(
    handler: &PH,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        "prompts/list" => {
            let result = handler.list_prompts(ctx).await;
            Some(result.map(|prompts| serde_json::json!({ "prompts": prompts })))
        }
        "prompts/get" => {
            let result = (|| async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params("prompts/get", "missing params")
                })?;
                let name = params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("prompts/get", "missing prompt name"))?;
                let args = params.get("arguments")
                    .and_then(|v| v.as_object())
                    .cloned();
                let result = handler.get_prompt(name, args, ctx).await?;
                Ok(serde_json::to_value(result).unwrap_or(serde_json::json!({})))
            })().await;
            Some(result)
        }
        _ => None,
    }
}

/// Macro to generate `RequestRouter` implementations for all handler combinations.
///
/// This macro reduces code duplication by generating all 2^3 = 8 combinations
/// of tool/resource/prompt handler registration states.
macro_rules! impl_request_router {
    // Base case: no handlers
    (base; $($bounds:tt)*) => {
        impl<H $($bounds)*> RequestRouter for Server<H, NotRegistered, NotRegistered, NotRegistered, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                _params: Option<&serde_json::Value>,
                _ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                match method {
                    "ping" => Ok(serde_json::json!({})),
                    _ => Err(McpError::method_not_found(method)),
                }
            }
        }
    };

    // Tools only
    (tools; $($bounds:tt)*) => {
        impl<H, TH $($bounds)*> RequestRouter for Server<H, Registered<TH>, NotRegistered, NotRegistered, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            TH: ToolHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_tools(self.tool_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };

    // Resources only
    (resources; $($bounds:tt)*) => {
        impl<H, RH $($bounds)*> RequestRouter for Server<H, NotRegistered, Registered<RH>, NotRegistered, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            RH: ResourceHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_resources(self.resource_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };

    // Prompts only
    (prompts; $($bounds:tt)*) => {
        impl<H, PH $($bounds)*> RequestRouter for Server<H, NotRegistered, NotRegistered, Registered<PH>, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            PH: PromptHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_prompts(self.prompt_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };

    // Tools + Resources
    (tools_resources; $($bounds:tt)*) => {
        impl<H, TH, RH $($bounds)*> RequestRouter for Server<H, Registered<TH>, Registered<RH>, NotRegistered, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            TH: ToolHandler + Send + Sync,
            RH: ResourceHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_tools(self.tool_handler(), method, params, ctx).await {
                    return result;
                }
                if let Some(result) = route_resources(self.resource_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };

    // Tools + Prompts
    (tools_prompts; $($bounds:tt)*) => {
        impl<H, TH, PH $($bounds)*> RequestRouter for Server<H, Registered<TH>, NotRegistered, Registered<PH>, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            TH: ToolHandler + Send + Sync,
            PH: PromptHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_tools(self.tool_handler(), method, params, ctx).await {
                    return result;
                }
                if let Some(result) = route_prompts(self.prompt_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };

    // Resources + Prompts
    (resources_prompts; $($bounds:tt)*) => {
        impl<H, RH, PH $($bounds)*> RequestRouter for Server<H, NotRegistered, Registered<RH>, Registered<PH>, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            RH: ResourceHandler + Send + Sync,
            PH: PromptHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_resources(self.resource_handler(), method, params, ctx).await {
                    return result;
                }
                if let Some(result) = route_prompts(self.prompt_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };

    // Tools + Resources + Prompts
    (tools_resources_prompts; $($bounds:tt)*) => {
        impl<H, TH, RH, PH $($bounds)*> RequestRouter for Server<H, Registered<TH>, Registered<RH>, Registered<PH>, NotRegistered>
        where
            H: ServerHandler + Send + Sync,
            TH: ToolHandler + Send + Sync,
            RH: ResourceHandler + Send + Sync,
            PH: PromptHandler + Send + Sync,
        {
            async fn route(
                &self,
                method: &str,
                params: Option<&serde_json::Value>,
                ctx: &Context<'_>,
            ) -> Result<serde_json::Value, McpError> {
                if method == "ping" {
                    return Ok(serde_json::json!({}));
                }
                if let Some(result) = route_tools(self.tool_handler(), method, params, ctx).await {
                    return result;
                }
                if let Some(result) = route_resources(self.resource_handler(), method, params, ctx).await {
                    return result;
                }
                if let Some(result) = route_prompts(self.prompt_handler(), method, params, ctx).await {
                    return result;
                }
                Err(McpError::method_not_found(method))
            }
        }
    };
}

// Generate all RequestRouter implementations
impl_request_router!(base;);
impl_request_router!(tools;);
impl_request_router!(resources;);
impl_request_router!(prompts;);
impl_request_router!(tools_resources;);
impl_request_router!(tools_prompts;);
impl_request_router!(resources_prompts;);
impl_request_router!(tools_resources_prompts;);

// ============================================================================
// Helper functions
// ============================================================================

/// Extract a progress token from request parameters.
///
/// Per the MCP specification, progress tokens are sent in the `_meta.progressToken`
/// field of request parameters. This function attempts to extract and parse that
/// field into a `ProgressToken`.
///
/// # Example JSON structure
/// ```json
/// {
///   "_meta": {
///     "progressToken": "token-123"
///   },
///   "name": "my-tool",
///   "arguments": {}
/// }
/// ```
fn extract_progress_token(params: Option<&serde_json::Value>) -> Option<ProgressToken> {
    params?
        .get("_meta")?
        .get("progressToken")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_initialization() {
        let state = ServerState::new(ServerCapabilities::default());
        assert!(!state.is_initialized());

        state.set_initialized();
        assert!(state.is_initialized());
    }

    #[test]
    fn test_cancellation_management() {
        let state = ServerState::new(ServerCapabilities::default());
        let token = CancellationToken::new();

        state.register_cancellation("req-1", token.clone());
        assert!(!token.is_cancelled());

        state.cancel_request("req-1");
        assert!(token.is_cancelled());

        state.remove_cancellation("req-1");
    }

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert!(config.auto_initialized);
        assert_eq!(config.max_concurrent_requests, 100);
    }

    #[test]
    fn test_extract_progress_token_string() {
        let params = serde_json::json!({
            "_meta": {
                "progressToken": "my-token-123"
            },
            "name": "test-tool"
        });
        let token = extract_progress_token(Some(&params));
        assert!(token.is_some());
        assert_eq!(token.unwrap(), ProgressToken::String("my-token-123".to_string()));
    }

    #[test]
    fn test_extract_progress_token_number() {
        let params = serde_json::json!({
            "_meta": {
                "progressToken": 42
            },
            "arguments": {}
        });
        let token = extract_progress_token(Some(&params));
        assert!(token.is_some());
        assert_eq!(token.unwrap(), ProgressToken::Number(42));
    }

    #[test]
    fn test_extract_progress_token_missing_meta() {
        let params = serde_json::json!({
            "name": "test-tool",
            "arguments": {}
        });
        let token = extract_progress_token(Some(&params));
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_progress_token_missing_token() {
        let params = serde_json::json!({
            "_meta": {},
            "name": "test-tool"
        });
        let token = extract_progress_token(Some(&params));
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_progress_token_none_params() {
        let token = extract_progress_token(None);
        assert!(token.is_none());
    }
}
