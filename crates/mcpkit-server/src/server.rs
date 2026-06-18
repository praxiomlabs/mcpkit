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
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::{Message, Notification, ProgressToken, Request, Response};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_core::types::CallToolResult;
use mcpkit_transport::Transport;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

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
    ///
    /// This is stored as a `ProtocolVersion` enum for type-safe feature detection.
    /// Use methods like `protocol_version().supports_tasks()` to check capabilities.
    pub negotiated_version: RwLock<Option<ProtocolVersion>>,
}

impl ServerState {
    /// Create a new server state.
    #[must_use]
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
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if let Some(version) = state.protocol_version() {
    ///     if version.supports_tasks() {
    ///         // Tasks are available in this session
    ///     }
    /// }
    /// ```
    pub fn protocol_version(&self) -> Option<ProtocolVersion> {
        self.negotiated_version.read().ok().and_then(|guard| *guard)
    }

    /// Set the negotiated protocol version.
    ///
    /// Silently fails if the lock is poisoned.
    pub fn set_protocol_version(&self, version: ProtocolVersion) {
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
    pub const fn new(transport: Arc<T>) -> Self {
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
                .map_err(std::convert::Into::into)
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
    /// Runtime configuration (concurrency limit, etc.).
    config: RuntimeConfig,
}

impl<S, Tr> ServerRuntime<S, Tr>
where
    S: RequestRouter + Send + Sync,
    Tr: Transport + 'static,
    Tr::Error: Into<McpError>,
{
    /// Get the server state.
    pub const fn state(&self) -> &Arc<ServerState> {
        &self.state
    }

    /// Run the server message loop.
    ///
    /// This method runs until the connection is closed or an error occurs.
    ///
    /// Requests are processed concurrently (interleaved on this task) up to
    /// `config.max_concurrent_requests` in flight at once; once that limit is
    /// reached, no new messages are accepted until an in-flight request
    /// completes (backpressure). Each request runs with panic isolation, so a
    /// panicking handler returns a JSON-RPC internal error instead of tearing
    /// down the connection. Notifications are handled inline.
    pub async fn run(&self) -> Result<(), McpError> {
        use futures::future::{Either, select};
        use futures::stream::{FuturesUnordered, StreamExt};

        let max = self.config.max_concurrent_requests.max(1);
        let mut in_flight = FuturesUnordered::new();

        let outcome = loop {
            // Obtain the next incoming message, making progress on in-flight
            // requests in the meantime. `in_flight.next()` is only awaited while
            // the set is non-empty, so it never spuriously yields `None`.
            let message = if in_flight.is_empty() {
                match self.transport.recv().await {
                    Ok(opt) => opt,
                    Err(e) => break Err(e.into()),
                }
            } else if in_flight.len() < max {
                // Race the next message against in-flight completions.
                let recv = std::pin::pin!(self.transport.recv());
                match select(recv, in_flight.next()).await {
                    Either::Left((Ok(opt), _)) => opt,
                    Either::Left((Err(e), _)) => break Err(e.into()),
                    // An in-flight request finished; its response was already
                    // sent. Loop to keep accepting work.
                    Either::Right((_, _)) => continue,
                }
            } else {
                // At the concurrency limit: drain one in-flight request before
                // accepting any new message (backpressure).
                in_flight.next().await;
                continue;
            };

            match message {
                Some(Message::Request(request)) => {
                    in_flight.push(self.handle_request_isolated(request));
                }
                Some(Message::Notification(notification)) => {
                    if let Err(e) = self.handle_notification(notification).await {
                        tracing::error!(error = %e, "Error handling notification");
                    }
                }
                Some(Message::Response(_)) => {
                    tracing::warn!("Received unexpected response message");
                }
                None => {
                    tracing::info!("Connection closed");
                    break Ok(());
                }
            }
        };

        // Drain any still-running requests so their responses are delivered
        // before we return.
        while in_flight.next().await.is_some() {}

        if let Err(ref err) = outcome {
            tracing::error!(error = %err, "Transport error");
        }
        outcome
    }

    /// Compute the result for a request without sending it.
    async fn compute_response(&self, request: &Request) -> Result<serde_json::Value, McpError> {
        match request.method.as_ref() {
            "initialize" => self.handle_initialize(request).await,
            _ if !self.state.is_initialized() => {
                Err(McpError::invalid_request("Server not initialized"))
            }
            _ => self.route_request(request).await,
        }
    }

    /// Handle a request with panic isolation, sending the response when done.
    ///
    /// A panic in the handler is caught and converted into a JSON-RPC internal
    /// error response so a single misbehaving handler cannot tear down the
    /// whole connection.
    async fn handle_request_isolated(&self, request: Request) {
        use futures::FutureExt;
        use std::panic::AssertUnwindSafe;

        let id = request.id.clone();
        tracing::debug!(method = %request.method, id = %id, "Handling request");

        let computed = AssertUnwindSafe(self.compute_response(&request))
            .catch_unwind()
            .await;

        let response_msg = match computed {
            Ok(Ok(result)) => Response::success(id, result),
            Ok(Err(e)) => Response::error(id, e.into()),
            Err(panic) => {
                let detail = panic_message(&*panic);
                tracing::error!(method = %request.method, panic = %detail, "Handler panicked");
                Response::error(
                    id,
                    McpError::internal(format!("handler panicked: {detail}")).into(),
                )
            }
        };

        if let Err(e) = self.transport.send(Message::Response(response_msg)).await {
            let err: McpError = e.into();
            tracing::error!(error = %err, "Failed to send response");
        }
    }

    /// Handle the initialize request.
    ///
    /// This performs protocol version negotiation according to the MCP specification:
    /// 1. Client sends its preferred protocol version
    /// 2. Server responds with the same version if supported, or its preferred version
    /// 3. Client must support the returned version or disconnect
    async fn handle_initialize(&self, request: &Request) -> Result<serde_json::Value, McpError> {
        if self.state.is_initialized() {
            return Err(McpError::invalid_request("Already initialized"));
        }

        // Parse initialize params
        let params = request
            .params
            .as_ref()
            .ok_or_else(|| McpError::invalid_params("initialize", "missing params"))?;

        // Extract and negotiate protocol version using type-safe enum
        let requested_version_str = params
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Negotiate using the ProtocolVersion enum for type safety
        let negotiated_version =
            ProtocolVersion::negotiate(requested_version_str, ProtocolVersion::ALL)
                .unwrap_or(ProtocolVersion::LATEST);

        // Log version negotiation details for debugging
        if requested_version_str == negotiated_version.as_str() {
            tracing::debug!(
                version = %negotiated_version,
                "Protocol version negotiated successfully"
            );
        } else {
            tracing::info!(
                requested = %requested_version_str,
                negotiated = %negotiated_version,
                supported = ?ProtocolVersion::ALL.iter().map(ProtocolVersion::as_str).collect::<Vec<_>>(),
                "Protocol version negotiation: client requested different version"
            );
        }

        // Store the negotiated version (type-safe enum)
        self.state.set_protocol_version(negotiated_version);

        // Extract client info and capabilities
        if let Some(caps) = params.get("capabilities") {
            if let Ok(client_caps) = serde_json::from_value::<ClientCapabilities>(caps.clone()) {
                self.state.set_client_caps(client_caps);
            }
        }

        // Build response with negotiated version (serialized to string by serde)
        let result = serde_json::json!({
            "protocolVersion": negotiated_version.as_str(),
            "serverInfo": self.server.server_info(),
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
        let protocol_version = self
            .state
            .protocol_version()
            .unwrap_or(ProtocolVersion::LATEST);
        let ctx = Context::new(
            &request.id,
            progress_token.as_ref(),
            &client_caps,
            &self.state.server_caps,
            protocol_version,
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
    pub fn with_config(
        server: Server<H, T, R, P, K>,
        transport: Tr,
        config: RuntimeConfig,
    ) -> Self {
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
    /// Get the server info.
    fn server_info(&self) -> mcpkit_core::capability::ServerInfo;

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
            tracing::debug!("Listing available tools");
            let result = handler.list_tools(ctx).await;
            match &result {
                Ok(tools) => tracing::debug!(count = tools.len(), "Listed tools"),
                Err(e) => tracing::warn!(error = %e, "Failed to list tools"),
            }
            Some(result.map(|tools| serde_json::json!({ "tools": tools })))
        }
        "tools/call" => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params("tools/call", "missing params")
                })?;
                let name = params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("tools/call", "missing tool name"))?;
                let args = params.get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                tracing::info!(tool = %name, "Calling tool");
                let start = std::time::Instant::now();
                let output = handler.call_tool(name, args, ctx).await;
                let duration = start.elapsed();

                match &output {
                    Ok(_) => tracing::info!(tool = %name, duration_ms = duration.as_millis(), "Tool call completed"),
                    Err(e) => tracing::warn!(tool = %name, duration_ms = duration.as_millis(), error = %e, "Tool call failed"),
                }

                let output = output?;
                let result: CallToolResult = output.into();
                Ok(serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
            }.await;
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
            tracing::debug!("Listing available resources");
            let result = handler.list_resources(ctx).await;
            match &result {
                Ok(resources) => tracing::debug!(count = resources.len(), "Listed resources"),
                Err(e) => tracing::warn!(error = %e, "Failed to list resources"),
            }
            Some(result.map(|resources| serde_json::json!({ "resources": resources })))
        }
        "resources/templates/list" => {
            tracing::debug!("Listing available resource templates");
            let result = handler.list_resource_templates(ctx).await;
            match &result {
                Ok(templates) => {
                    tracing::debug!(count = templates.len(), "Listed resource templates");
                }
                Err(e) => tracing::warn!(error = %e, "Failed to list resource templates"),
            }
            Some(result.map(|templates| serde_json::json!({ "resourceTemplates": templates })))
        }
        "resources/read" => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params("resources/read", "missing params")
                })?;
                let uri = params.get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("resources/read", "missing uri"))?;

                tracing::info!(uri = %uri, "Reading resource");
                let start = std::time::Instant::now();
                let contents = handler.read_resource(uri, ctx).await;
                let duration = start.elapsed();

                match &contents {
                    Ok(_) => tracing::info!(uri = %uri, duration_ms = duration.as_millis(), "Resource read completed"),
                    Err(e) => tracing::warn!(uri = %uri, duration_ms = duration.as_millis(), error = %e, "Resource read failed"),
                }

                let contents = contents?;
                Ok(serde_json::json!({ "contents": contents }))
            }.await;
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
            tracing::debug!("Listing available prompts");
            let result = handler.list_prompts(ctx).await;
            match &result {
                Ok(prompts) => tracing::debug!(count = prompts.len(), "Listed prompts"),
                Err(e) => tracing::warn!(error = %e, "Failed to list prompts"),
            }
            Some(result.map(|prompts| serde_json::json!({ "prompts": prompts })))
        }
        "prompts/get" => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params("prompts/get", "missing params")
                })?;
                let name = params.get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| McpError::invalid_params("prompts/get", "missing prompt name"))?;
                let args = params.get("arguments")
                    .and_then(|v| v.as_object())
                    .cloned();

                tracing::info!(prompt = %name, "Getting prompt");
                let start = std::time::Instant::now();
                let prompt_result = handler.get_prompt(name, args, ctx).await;
                let duration = start.elapsed();

                match &prompt_result {
                    Ok(_) => tracing::info!(prompt = %name, duration_ms = duration.as_millis(), "Prompt retrieval completed"),
                    Err(e) => tracing::warn!(prompt = %name, duration_ms = duration.as_millis(), error = %e, "Prompt retrieval failed"),
                }

                let result = prompt_result?;
                Ok(serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
            }.await;
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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
            fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
                self.handler().server_info()
            }

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
/// Extract a human-readable message from a caught panic payload.
fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

fn extract_progress_token(params: Option<&serde_json::Value>) -> Option<ProgressToken> {
    params?
        .get("_meta")?
        .get("progressToken")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    use mcpkit_core::capability::ServerInfo;
    use mcpkit_core::protocol::RequestId;
    use mcpkit_transport::MemoryTransport;
    use std::time::Duration;
    use tokio::sync::Notify;
    use tokio::time::timeout;

    /// A minimal router whose `route` can panic, succeed, or 404.
    struct PanicRouter;

    impl RequestRouter for PanicRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("panic-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            _ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "panic" => panic!("boom in handler"),
                "ok" => Ok(serde_json::json!("ok")),
                other => Err(McpError::method_not_found(other)),
            }
        }
    }

    /// A router that parks the "blocker" request until released, to prove
    /// requests are processed concurrently rather than serially.
    struct CoordRouter {
        started: Arc<Notify>,
        release: Arc<Notify>,
    }

    impl RequestRouter for CoordRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("coord-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            _ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "blocker" => {
                    self.started.notify_one();
                    self.release.notified().await;
                    Ok(serde_json::json!("blocked-done"))
                }
                "fast" => Ok(serde_json::json!("fast-done")),
                other => Err(McpError::method_not_found(other)),
            }
        }
    }

    fn req(method: &'static str, id: u64) -> Message {
        Message::Request(Request::new(method, id))
    }

    async fn next_response(transport: &MemoryTransport) -> Response {
        let msg = timeout(Duration::from_secs(2), transport.recv())
            .await
            .expect("no response (connection died?)")
            .expect("recv ok")
            .expect("some message");
        match msg {
            Message::Response(r) => r,
            other => panic!("expected response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn panic_in_handler_returns_internal_error_and_keeps_connection() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        let runtime = ServerRuntime {
            server: PanicRouter,
            transport: Arc::new(server),
            state,
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        // A panicking handler must yield a JSON-RPC error, not kill the loop.
        client.send(req("panic", 1)).await.expect("send");
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        let err = resp.error.expect("expected error response");
        assert!(
            err.message.contains("panicked"),
            "unexpected error message: {}",
            err.message
        );

        // The connection must still be alive for subsequent requests.
        client.send(req("ok", 2)).await.expect("send");
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(2));
        assert!(
            resp.result.is_some(),
            "expected success after a prior panic"
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn requests_are_processed_concurrently() {
        let (client, server) = MemoryTransport::pair();
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        let runtime = ServerRuntime {
            server: CoordRouter {
                started: started.clone(),
                release: release.clone(),
            },
            transport: Arc::new(server),
            state,
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("blocker", 1)).await.expect("send");
        client.send(req("fast", 2)).await.expect("send");

        // Wait until the blocker is in-flight and parked.
        timeout(Duration::from_secs(2), started.notified())
            .await
            .expect("blocker never started");

        // If processing were serial, the parked blocker would prevent the fast
        // request from completing. Concurrency means the fast response (id 2)
        // arrives while the blocker is still parked.
        let resp = next_response(&client).await;
        assert_eq!(
            resp.id,
            RequestId::Number(2),
            "fast request should finish first"
        );

        // Release the blocker; its response should now arrive.
        release.notify_one();
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn max_concurrent_requests_limits_in_flight() {
        // With a limit of 1, a parked blocker must prevent a second request
        // from being picked up until the blocker completes.
        let (client, server) = MemoryTransport::pair();
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        let runtime = ServerRuntime {
            server: CoordRouter {
                started: started.clone(),
                release: release.clone(),
            },
            transport: Arc::new(server),
            state,
            config: RuntimeConfig {
                auto_initialized: true,
                max_concurrent_requests: 1,
            },
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("blocker", 1)).await.expect("send");
        client.send(req("fast", 2)).await.expect("send");

        timeout(Duration::from_secs(2), started.notified())
            .await
            .expect("blocker never started");

        // The fast request must NOT be processed while the blocker holds the
        // single slot: no response should arrive yet.
        let early = timeout(Duration::from_millis(200), client.recv()).await;
        assert!(
            early.is_err(),
            "fast request was processed despite max_concurrent_requests = 1"
        );

        // Release the blocker; both responses arrive, blocker first.
        release.notify_one();
        assert_eq!(next_response(&client).await.id, RequestId::Number(1));
        assert_eq!(next_response(&client).await.id, RequestId::Number(2));

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

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
    fn test_extract_progress_token_string() -> Result<(), Box<dyn std::error::Error>> {
        let params = serde_json::json!({
            "_meta": {
                "progressToken": "my-token-123"
            },
            "name": "test-tool"
        });
        let token = extract_progress_token(Some(&params));
        assert!(token.is_some());
        assert_eq!(
            token.ok_or("Token not found")?,
            ProgressToken::String("my-token-123".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_extract_progress_token_number() -> Result<(), Box<dyn std::error::Error>> {
        let params = serde_json::json!({
            "_meta": {
                "progressToken": 42
            },
            "arguments": {}
        });
        let token = extract_progress_token(Some(&params));
        assert!(token.is_some());
        assert_eq!(token.ok_or("Token not found")?, ProgressToken::Number(42));

        Ok(())
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
