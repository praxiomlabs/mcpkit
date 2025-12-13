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
//! - Server-initiated request handling via [`ClientHandler`]

use futures::channel::oneshot;
use mcpkit_core::capability::{
    is_version_supported, ClientCapabilities, ClientInfo, InitializeRequest, InitializeResult,
    ServerCapabilities, ServerInfo, PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
};
use mcpkit_core::error::{
    HandshakeDetails, JsonRpcError, McpError, TransportContext, TransportDetails,
    TransportErrorKind,
};
use mcpkit_core::protocol::{Message, Notification, Request, RequestId, Response};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_core::types::{
    CallToolRequest, CallToolResult, CancelTaskRequest, CompleteRequest, CompleteResult,
    CompletionArgument, CompletionRef, CreateMessageRequest, ElicitRequest, GetPromptRequest,
    GetPromptResult, GetTaskRequest, ListPromptsResult, ListResourceTemplatesResult,
    ListResourcesResult, ListTasksRequest, ListTasksResult, ListToolsResult, Prompt,
    ReadResourceRequest, ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Task,
    TaskStatus, TaskSummary, Tool,
};
use mcpkit_transport::Transport;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

// Runtime-agnostic sync primitives
use async_lock::RwLock;

// Use tokio channels when tokio-runtime is enabled, otherwise use the transport abstraction
#[cfg(feature = "tokio-runtime")]
use tokio::sync::mpsc;

use crate::handler::ClientHandler;

/// An MCP client connected to a server.
///
/// The client provides methods for interacting with MCP servers:
///
/// - Tools: `list_tools()`, `call_tool()`
/// - Resources: `list_resources()`, `read_resource()`
/// - Prompts: `list_prompts()`, `get_prompt()`
/// - Tasks: `list_tasks()`, `get_task()`, `cancel_task()`
///
/// The client also handles server-initiated requests (sampling, elicitation)
/// by delegating to a [`ClientHandler`] implementation.
///
/// # Example
///
/// ```no_run
/// use mcpkit_client::ClientBuilder;
/// use mcpkit_transport::SpawnedTransport;
///
/// # async fn example() -> Result<(), mcpkit_core::error::McpError> {
/// let transport = SpawnedTransport::spawn("my-server", &[] as &[&str]).await?;
/// let client = ClientBuilder::new()
///     .name("my-client")
///     .version("1.0.0")
///     .build(transport)
///     .await?;
///
/// let tools = client.list_tools().await?;
/// # Ok(())
/// # }
/// ```
pub struct Client<T: Transport, H: ClientHandler = crate::handler::NoOpHandler> {
    /// The underlying transport (shared with background task).
    transport: Arc<T>,
    /// Server information received during initialization.
    server_info: ServerInfo,
    /// Server capabilities.
    server_caps: ServerCapabilities,
    /// Negotiated protocol version.
    ///
    /// Use this for feature detection via methods like `supports_tasks()`,
    /// `supports_elicitation()`, etc.
    protocol_version: ProtocolVersion,
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
    /// Handler for server-initiated requests.
    handler: Arc<H>,
    /// Sender for outgoing messages to the background task.
    outgoing_tx: mpsc::Sender<Message>,
    /// Flag indicating if the client is running.
    running: Arc<AtomicBool>,
    /// Handle to the background task.
    _background_handle: Option<tokio::task::JoinHandle<()>>,
}

impl<T: Transport + 'static> Client<T, crate::handler::NoOpHandler> {
    /// Create a new client without a handler (called by builder).
    pub(crate) fn new(
        transport: T,
        init_result: InitializeResult,
        client_info: ClientInfo,
        client_caps: ClientCapabilities,
    ) -> Self {
        Self::with_handler(
            transport,
            init_result,
            client_info,
            client_caps,
            crate::handler::NoOpHandler,
        )
    }
}

impl<T: Transport + 'static, H: ClientHandler + 'static> Client<T, H> {
    /// Create a new client with a custom handler (called by builder).
    pub(crate) fn with_handler(
        transport: T,
        init_result: InitializeResult,
        client_info: ClientInfo,
        client_caps: ClientCapabilities,
        handler: H,
    ) -> Self {
        let transport = Arc::new(transport);
        let pending = Arc::new(RwLock::new(HashMap::new()));
        let handler = Arc::new(handler);
        let running = Arc::new(AtomicBool::new(true));

        // Parse the negotiated protocol version
        let protocol_version =
            if let Ok(v) = init_result.protocol_version.parse::<ProtocolVersion>() {
                v
            } else {
                warn!(
                    server_version = %init_result.protocol_version,
                    fallback_version = %ProtocolVersion::LATEST,
                    "Server returned unknown protocol version, falling back to latest supported"
                );
                ProtocolVersion::LATEST
            };

        // Create channel for outgoing messages
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<Message>(256);

        // Start background message routing task
        let background_handle = Self::spawn_message_router(
            Arc::clone(&transport),
            Arc::clone(&pending),
            Arc::clone(&handler),
            Arc::clone(&running),
            outgoing_rx,
        );

        // Notify handler that connection is established
        let handler_clone = Arc::clone(&handler);
        tokio::spawn(async move {
            handler_clone.on_connected().await;
        });

        Self {
            transport,
            server_info: init_result.server_info,
            server_caps: init_result.capabilities,
            protocol_version,
            client_info,
            client_caps,
            next_id: AtomicU64::new(1),
            pending,
            instructions: init_result.instructions,
            handler,
            outgoing_tx,
            running,
            _background_handle: Some(background_handle),
        }
    }

    /// Spawn the background message routing task.
    ///
    /// This task:
    /// - Reads incoming messages from the transport
    /// - Routes responses to pending request channels
    /// - Delegates server-initiated requests to the handler
    /// - Handles notifications
    fn spawn_message_router(
        transport: Arc<T>,
        pending: Arc<RwLock<HashMap<RequestId, oneshot::Sender<Response>>>>,
        handler: Arc<H>,
        running: Arc<AtomicBool>,
        mut outgoing_rx: mpsc::Receiver<Message>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            debug!("Starting client message router");

            loop {
                if !running.load(Ordering::SeqCst) {
                    debug!("Message router stopping (client closed)");
                    break;
                }

                tokio::select! {
                    // Handle outgoing messages
                    Some(msg) = outgoing_rx.recv() => {
                        if let Err(e) = transport.send(msg).await {
                            error!(?e, "Failed to send message");
                        }
                    }

                    // Handle incoming messages
                    result = transport.recv() => {
                        match result {
                            Ok(Some(message)) => {
                                Self::handle_incoming_message(
                                    message,
                                    &pending,
                                    &handler,
                                    &transport,
                                ).await;
                            }
                            Ok(None) => {
                                info!("Connection closed by server");
                                running.store(false, Ordering::SeqCst);
                                handler.on_disconnected().await;
                                break;
                            }
                            Err(e) => {
                                error!(?e, "Transport error in message router");
                                running.store(false, Ordering::SeqCst);
                                handler.on_disconnected().await;
                                break;
                            }
                        }
                    }
                }
            }

            debug!("Message router stopped");
        })
    }

    /// Handle an incoming message from the server.
    async fn handle_incoming_message(
        message: Message,
        pending: &Arc<RwLock<HashMap<RequestId, oneshot::Sender<Response>>>>,
        handler: &Arc<H>,
        transport: &Arc<T>,
    ) {
        match message {
            Message::Response(response) => {
                Self::route_response(response, pending).await;
            }
            Message::Request(request) => {
                Self::handle_server_request(request, handler, transport).await;
            }
            Message::Notification(notification) => {
                Self::handle_notification(notification, handler).await;
            }
        }
    }

    /// Route a response to the appropriate pending request.
    async fn route_response(
        response: Response,
        pending: &Arc<RwLock<HashMap<RequestId, oneshot::Sender<Response>>>>,
    ) {
        let sender = {
            let mut pending_guard = pending.write().await;
            pending_guard.remove(&response.id)
        };

        if let Some(sender) = sender {
            trace!(?response.id, "Routing response to pending request");
            if sender.send(response).is_err() {
                warn!("Pending request receiver dropped");
            }
        } else {
            warn!(?response.id, "Received response for unknown request");
        }
    }

    /// Handle a server-initiated request.
    async fn handle_server_request(request: Request, handler: &Arc<H>, transport: &Arc<T>) {
        trace!(method = %request.method, "Handling server request");

        let response = match request.method.as_ref() {
            "sampling/createMessage" => Self::handle_sampling_request(&request, handler).await,
            "elicitation/elicit" => Self::handle_elicitation_request(&request, handler).await,
            "roots/list" => Self::handle_roots_request(&request, handler).await,
            "ping" => {
                // Respond to ping with empty result
                Response::success(request.id.clone(), serde_json::json!({}))
            }
            _ => {
                warn!(method = %request.method, "Unknown server request method");
                Response::error(
                    request.id.clone(),
                    JsonRpcError::method_not_found(format!("Unknown method: {}", request.method)),
                )
            }
        };

        // Send the response
        if let Err(e) = transport.send(Message::Response(response)).await {
            error!(?e, "Failed to send response to server request");
        }
    }

    /// Handle a sampling/createMessage request.
    async fn handle_sampling_request(request: &Request, handler: &Arc<H>) -> Response {
        let params = match &request.params {
            Some(p) => match serde_json::from_value::<CreateMessageRequest>(p.clone()) {
                Ok(req) => req,
                Err(e) => {
                    return Response::error(
                        request.id.clone(),
                        JsonRpcError::invalid_params(format!("Invalid params: {e}")),
                    );
                }
            },
            None => {
                return Response::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params("Missing params for sampling/createMessage"),
                );
            }
        };

        match handler.create_message(params).await {
            Ok(result) => match serde_json::to_value(result) {
                Ok(value) => Response::success(request.id.clone(), value),
                Err(e) => Response::error(
                    request.id.clone(),
                    JsonRpcError::internal_error(format!("Serialization error: {e}")),
                ),
            },
            Err(e) => Response::error(
                request.id.clone(),
                JsonRpcError::internal_error(e.to_string()),
            ),
        }
    }

    /// Handle an elicitation/elicit request.
    async fn handle_elicitation_request(request: &Request, handler: &Arc<H>) -> Response {
        let params = match &request.params {
            Some(p) => match serde_json::from_value::<ElicitRequest>(p.clone()) {
                Ok(req) => req,
                Err(e) => {
                    return Response::error(
                        request.id.clone(),
                        JsonRpcError::invalid_params(format!("Invalid params: {e}")),
                    );
                }
            },
            None => {
                return Response::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params("Missing params for elicitation/elicit"),
                );
            }
        };

        match handler.elicit(params).await {
            Ok(result) => match serde_json::to_value(result) {
                Ok(value) => Response::success(request.id.clone(), value),
                Err(e) => Response::error(
                    request.id.clone(),
                    JsonRpcError::internal_error(format!("Serialization error: {e}")),
                ),
            },
            Err(e) => Response::error(
                request.id.clone(),
                JsonRpcError::internal_error(e.to_string()),
            ),
        }
    }

    /// Handle a roots/list request.
    async fn handle_roots_request(request: &Request, handler: &Arc<H>) -> Response {
        match handler.list_roots().await {
            Ok(roots) => {
                let roots_json: Vec<serde_json::Value> = roots
                    .into_iter()
                    .map(|r| {
                        serde_json::json!({
                            "uri": r.uri,
                            "name": r.name
                        })
                    })
                    .collect();
                Response::success(
                    request.id.clone(),
                    serde_json::json!({ "roots": roots_json }),
                )
            }
            Err(e) => Response::error(
                request.id.clone(),
                JsonRpcError::internal_error(e.to_string()),
            ),
        }
    }

    /// Handle a notification from the server.
    async fn handle_notification(notification: Notification, handler: &Arc<H>) {
        trace!(method = %notification.method, "Received server notification");

        match notification.method.as_ref() {
            "notifications/cancelled" => {
                // Handle cancellation notifications
                if let Some(params) = &notification.params {
                    if let Some(request_id) = params.get("requestId") {
                        debug!(?request_id, "Server cancelled request");
                    }
                }
            }
            "notifications/progress" => {
                // Handle progress notifications
                if let Some(params) = notification.params {
                    if let (Some(task_id), Some(progress)) = (
                        params.get("progressToken").and_then(|v| v.as_str()),
                        params.get("progress"),
                    ) {
                        if let Ok(progress) = serde_json::from_value::<
                            mcpkit_core::types::TaskProgress,
                        >(progress.clone())
                        {
                            debug!(task_id = %task_id, "Task progress update");
                            handler.on_task_progress(task_id.into(), progress).await;
                        }
                    }
                }
            }
            "notifications/resources/updated" => {
                if let Some(params) = notification.params {
                    if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                        debug!(uri = %uri, "Resource updated");
                        handler.on_resource_updated(uri.to_string()).await;
                    }
                }
            }
            "notifications/resources/list_changed" => {
                debug!("Resources list changed");
                handler.on_resources_list_changed().await;
            }
            "notifications/tools/list_changed" => {
                debug!("Tools list changed");
                handler.on_tools_list_changed().await;
            }
            "notifications/prompts/list_changed" => {
                debug!("Prompts list changed");
                handler.on_prompts_list_changed().await;
            }
            _ => {
                trace!(method = %notification.method, "Unhandled notification");
            }
        }
    }

    /// Get the server information.
    pub const fn server_info(&self) -> &ServerInfo {
        &self.server_info
    }

    /// Get the server capabilities.
    pub const fn server_capabilities(&self) -> &ServerCapabilities {
        &self.server_caps
    }

    /// Get the negotiated protocol version.
    ///
    /// Use this for feature detection. For example:
    /// ```rust,ignore
    /// if client.protocol_version().supports_tasks() {
    ///     // Use task-related features
    /// }
    /// ```
    pub fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Get the client information.
    pub const fn client_info(&self) -> &ClientInfo {
        &self.client_info
    }

    /// Get the client capabilities.
    pub const fn client_capabilities(&self) -> &ClientCapabilities {
        &self.client_caps
    }

    /// Get the server instructions, if provided.
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Check if the server supports tools.
    pub const fn has_tools(&self) -> bool {
        self.server_caps.has_tools()
    }

    /// Check if the server supports resources.
    pub const fn has_resources(&self) -> bool {
        self.server_caps.has_resources()
    }

    /// Check if the server supports prompts.
    pub const fn has_prompts(&self) -> bool {
        self.server_caps.has_prompts()
    }

    /// Check if the server supports tasks.
    pub const fn has_tasks(&self) -> bool {
        self.server_caps.has_tasks()
    }

    /// Check if the server supports completions.
    pub const fn has_completions(&self) -> bool {
        self.server_caps.has_completions()
    }

    /// Check if the client is still connected.
    pub fn is_connected(&self) -> bool {
        self.running.load(Ordering::SeqCst)
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
    pub async fn read_resource(
        &self,
        uri: impl Into<String>,
    ) -> Result<Vec<ResourceContents>, McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        let request = ReadResourceRequest { uri: uri.into() };
        let result: ReadResourceResult = self
            .request("resources/read", Some(serde_json::to_value(request)?))
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
    // Task Operations
    // ==========================================================================

    /// List all tasks.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks are not supported or the request fails.
    pub async fn list_tasks(&self) -> Result<Vec<TaskSummary>, McpError> {
        self.ensure_capability("tasks", self.has_tasks())?;

        let result: ListTasksResult = self.request("tasks/list", None).await?;
        Ok(result.tasks)
    }

    /// List tasks with optional status filter and pagination.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks are not supported or the request fails.
    pub async fn list_tasks_filtered(
        &self,
        status: Option<TaskStatus>,
        cursor: Option<&str>,
    ) -> Result<ListTasksResult, McpError> {
        self.ensure_capability("tasks", self.has_tasks())?;

        let request = ListTasksRequest {
            status,
            cursor: cursor.map(String::from),
        };
        self.request("tasks/list", Some(serde_json::to_value(request)?))
            .await
    }

    /// Get a task by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks are not supported or the task is not found.
    pub async fn get_task(&self, id: impl Into<String>) -> Result<Task, McpError> {
        self.ensure_capability("tasks", self.has_tasks())?;

        let request = GetTaskRequest {
            id: id.into().into(),
        };
        self.request("tasks/get", Some(serde_json::to_value(request)?))
            .await
    }

    /// Cancel a running task.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks are not supported, cancellation is not supported,
    /// or the task is not found.
    pub async fn cancel_task(&self, id: impl Into<String>) -> Result<(), McpError> {
        self.ensure_capability("tasks", self.has_tasks())?;

        let request = CancelTaskRequest {
            id: id.into().into(),
        };
        let _: serde_json::Value = self
            .request("tasks/cancel", Some(serde_json::to_value(request)?))
            .await?;
        Ok(())
    }

    // ==========================================================================
    // Completion Operations
    // ==========================================================================

    /// Get completions for a prompt argument.
    ///
    /// # Arguments
    ///
    /// * `prompt_name` - The name of the prompt
    /// * `argument_name` - The name of the argument to complete
    /// * `current_value` - The current partial value being typed
    ///
    /// # Errors
    ///
    /// Returns an error if completions are not supported or the request fails.
    pub async fn complete_prompt_argument(
        &self,
        prompt_name: impl Into<String>,
        argument_name: impl Into<String>,
        current_value: impl Into<String>,
    ) -> Result<CompleteResult, McpError> {
        self.ensure_capability("completions", self.has_completions())?;

        let request = CompleteRequest {
            ref_: CompletionRef::prompt(prompt_name),
            argument: CompletionArgument {
                name: argument_name.into(),
                value: current_value.into(),
            },
        };
        self.request("completion/complete", Some(serde_json::to_value(request)?))
            .await
    }

    /// Get completions for a resource argument.
    ///
    /// # Arguments
    ///
    /// * `resource_uri` - The URI of the resource
    /// * `argument_name` - The name of the argument to complete
    /// * `current_value` - The current partial value being typed
    ///
    /// # Errors
    ///
    /// Returns an error if completions are not supported or the request fails.
    pub async fn complete_resource_argument(
        &self,
        resource_uri: impl Into<String>,
        argument_name: impl Into<String>,
        current_value: impl Into<String>,
    ) -> Result<CompleteResult, McpError> {
        self.ensure_capability("completions", self.has_completions())?;

        let request = CompleteRequest {
            ref_: CompletionRef::resource(resource_uri),
            argument: CompletionArgument {
                name: argument_name.into(),
                value: current_value.into(),
            },
        };
        self.request("completion/complete", Some(serde_json::to_value(request)?))
            .await
    }

    // ==========================================================================
    // Resource Subscription Operations
    // ==========================================================================

    /// Subscribe to updates for a resource.
    ///
    /// When subscribed, the server will send `notifications/resources/updated`
    /// when the resource changes.
    ///
    /// # Errors
    ///
    /// Returns an error if resource subscriptions are not supported or the request fails.
    pub async fn subscribe_resource(&self, uri: impl Into<String>) -> Result<(), McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        // Check if subscribe is supported
        if !self.server_caps.has_resource_subscribe() {
            return Err(McpError::CapabilityNotSupported {
                capability: "resources.subscribe".to_string(),
                available: self.available_capabilities().into_boxed_slice(),
            });
        }

        let params = serde_json::json!({ "uri": uri.into() });
        let _: serde_json::Value = self.request("resources/subscribe", Some(params)).await?;
        Ok(())
    }

    /// Unsubscribe from updates for a resource.
    ///
    /// # Errors
    ///
    /// Returns an error if resource subscriptions are not supported or the request fails.
    pub async fn unsubscribe_resource(&self, uri: impl Into<String>) -> Result<(), McpError> {
        self.ensure_capability("resources", self.has_resources())?;

        // Check if subscribe is supported
        if !self.server_caps.has_resource_subscribe() {
            return Err(McpError::CapabilityNotSupported {
                capability: "resources.subscribe".to_string(),
                available: self.available_capabilities().into_boxed_slice(),
            });
        }

        let params = serde_json::json!({ "uri": uri.into() });
        let _: serde_json::Value = self.request("resources/unsubscribe", Some(params)).await?;
        Ok(())
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

        // Signal the background task to stop
        self.running.store(false, Ordering::SeqCst);

        // Notify handler
        self.handler.on_disconnected().await;

        // Close the transport
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
        if !self.is_connected() {
            return Err(McpError::Transport(Box::new(TransportDetails {
                kind: TransportErrorKind::ConnectionClosed,
                message: "Client is not connected".to_string(),
                context: TransportContext::default(),
                source: None,
            })));
        }

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

        // Send the request through the outgoing channel
        self.outgoing_tx
            .send(Message::Request(request))
            .await
            .map_err(|_| {
                McpError::Transport(Box::new(TransportDetails {
                    kind: TransportErrorKind::WriteFailed,
                    message: "Failed to send request (channel closed)".to_string(),
                    context: TransportContext::default(),
                    source: None,
                }))
            })?;

        // Wait for the response with a timeout
        let response = rx.await.map_err(|_| {
            McpError::Transport(Box::new(TransportDetails {
                kind: TransportErrorKind::ConnectionClosed,
                message: "Response channel closed (server may have disconnected)".to_string(),
                context: TransportContext::default(),
                source: None,
            }))
        })?;

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
        if self.has_completions() {
            caps.push("completions".to_string());
        }
        caps
    }
}

/// Initialize a client connection.
///
/// This performs the MCP handshake with protocol version negotiation:
/// 1. Send initialize request with our preferred protocol version
/// 2. Wait for initialize result with server's negotiated version
/// 3. Validate we support the server's version (disconnect if not)
/// 4. Send initialized notification
///
/// # Protocol Version Negotiation
///
/// Per the MCP specification:
/// - Client sends its preferred (latest) protocol version
/// - Server responds with the same version if supported, or its own preferred version
/// - Client must support the server's version or the handshake fails
///
/// This SDK supports protocol versions: `2025-11-25`, `2024-11-05`.
pub(crate) async fn initialize<T: Transport>(
    transport: &T,
    client_info: &ClientInfo,
    capabilities: &ClientCapabilities,
) -> Result<InitializeResult, McpError> {
    debug!(
        protocol_version = %PROTOCOL_VERSION,
        supported_versions = ?SUPPORTED_PROTOCOL_VERSIONS,
        "Initializing MCP connection"
    );

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
        .map_err(|e| {
            McpError::Transport(Box::new(TransportDetails {
                kind: TransportErrorKind::WriteFailed,
                message: format!("Failed to send initialize: {e}"),
                context: TransportContext::default(),
                source: None,
            }))
        })?;

    // Wait for response
    let response = loop {
        match transport.recv().await {
            Ok(Some(Message::Response(r))) if r.id == RequestId::Number(0) => break r,
            Ok(Some(_)) => {} // Skip non-matching messages
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
        .ok_or_else(|| {
            McpError::HandshakeFailed(Box::new(HandshakeDetails {
                message: "Empty initialize result".to_string(),
                client_version: Some(PROTOCOL_VERSION.to_string()),
                server_version: None,
                source: None,
            }))
        })?;

    // Validate protocol version
    let server_version = &result.protocol_version;
    if !is_version_supported(server_version) {
        warn!(
            server_version = %server_version,
            supported = ?SUPPORTED_PROTOCOL_VERSIONS,
            "Server returned unsupported protocol version"
        );
        return Err(McpError::HandshakeFailed(Box::new(HandshakeDetails {
            message: format!(
                "Unsupported protocol version: server returned '{server_version}', but client only supports {SUPPORTED_PROTOCOL_VERSIONS:?}"
            ),
            client_version: Some(PROTOCOL_VERSION.to_string()),
            server_version: Some(server_version.clone()),
            source: None,
        })));
    }

    debug!(
        server = %result.server_info.name,
        server_version = %result.server_info.version,
        protocol_version = %result.protocol_version,
        "Received initialize result with compatible protocol version"
    );

    // Send initialized notification
    let notification = Notification::new("notifications/initialized");
    transport
        .send(Message::Notification(notification))
        .await
        .map_err(|e| {
            McpError::Transport(Box::new(TransportDetails {
                kind: TransportErrorKind::WriteFailed,
                message: format!("Failed to send initialized: {e}"),
                context: TransportContext::default(),
                source: None,
            }))
        })?;

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
