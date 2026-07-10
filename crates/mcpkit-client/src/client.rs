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
    ClientCapabilities, ClientInfo, InitializeRequest, InitializeResult, PROTOCOL_VERSION,
    SUPPORTED_PROTOCOL_VERSIONS, ServerCapabilities, ServerInfo, is_version_supported,
};
use mcpkit_core::error::{
    HandshakeDetails, JsonRpcError, McpError, TransportContext, TransportDetails,
    TransportErrorKind,
};
use mcpkit_core::protocol::{Message, Notification, Request, RequestId, Response};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_core::types::{
    CallToolRequest, CallToolResult, CancelTaskRequest, CancelTaskResult, CompleteRequest,
    CompleteResult, CompletionArgument, CompletionRef, CreateMessageRequest, ElicitRequestParams,
    GetPromptRequest, GetPromptResult, GetTaskRequest, GetTaskResult, ListPromptsResult,
    ListResourceTemplatesResult, ListResourcesResult, ListTasksRequest, ListTasksResult,
    ListToolsResult, Prompt, ReadResourceRequest, ReadResourceResult, Resource, ResourceContents,
    ResourceTemplate, SubscribeRequest, Task, TaskStatus, Tool, UnsubscribeRequest,
};
use mcpkit_transport::Transport;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, error, info, trace, warn};

// Runtime-agnostic sync primitives
use async_lock::RwLock;

// Use tokio channels when tokio-runtime is enabled, otherwise use the transport abstraction
#[cfg(feature = "tokio-runtime")]
use tokio::sync::mpsc;

use crate::handler::{ClientHandler, RequestContext};
use mcpkit_core::tasks::{TaskManager, route_task_store};

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
    /// Maximum time to wait for a response to a request before timing out.
    request_timeout: Duration,
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
        request_timeout: Duration,
    ) -> Self {
        Self::with_handler(
            transport,
            init_result,
            client_info,
            client_caps,
            crate::handler::NoOpHandler,
            request_timeout,
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
        request_timeout: Duration,
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

        // Per-connection task store for task-augmented server->client requests
        // (only when the client declared the `tasks` capability).
        let tasks = client_caps
            .tasks
            .is_some()
            .then(|| Arc::new(TaskManager::new()));

        // Start background message routing task
        let background_handle = Self::spawn_message_router(
            Arc::clone(&transport),
            Arc::clone(&pending),
            Arc::clone(&handler),
            Arc::clone(&running),
            outgoing_rx,
            Arc::new(client_caps.clone()),
            tasks,
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
            request_timeout,
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
        client_caps: Arc<ClientCapabilities>,
        tasks: Option<Arc<TaskManager>>,
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
                                // Debug: log what was received
                                let msg_id = match &message {
                                    Message::Request(r) => format!("Request({})", r.id),
                                    Message::Response(r) => format!("Response({})", r.id),
                                    Message::Notification(n) => format!("Notification({})", n.method),
                                };
                                debug!(msg = %msg_id, "Router received message from transport");
                                Self::handle_incoming_message(
                                    message,
                                    &pending,
                                    &handler,
                                    &transport,
                                    &client_caps,
                                    tasks.as_ref(),
                                ).await;
                            }
                            Ok(None) => {
                                info!("Connection closed by server");
                                running.store(false, Ordering::SeqCst);
                                // Drop pending senders so in-flight requests fail
                                // fast instead of waiting out their timeout.
                                pending.write().await.clear();
                                handler.on_disconnected().await;
                                break;
                            }
                            Err(e) => {
                                error!(?e, "Transport error in message router");
                                running.store(false, Ordering::SeqCst);
                                // Drop pending senders so in-flight requests fail
                                // fast instead of waiting out their timeout.
                                pending.write().await.clear();
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
        client_caps: &Arc<ClientCapabilities>,
        tasks: Option<&Arc<TaskManager>>,
    ) {
        match message {
            Message::Response(response) => {
                Self::route_response(response, pending).await;
            }
            Message::Request(request) => {
                // Handle off the router loop: a slow handler (or a spec-blocking
                // `tasks/result`, which waits for the task to reach a terminal
                // status) must not stall response routing or the `tasks/cancel`
                // that would unblock it.
                let handler = Arc::clone(handler);
                let transport = Arc::clone(transport);
                let client_caps = Arc::clone(client_caps);
                let tasks = tasks.cloned();
                tokio::spawn(async move {
                    Self::handle_server_request(
                        request,
                        &handler,
                        &transport,
                        &client_caps,
                        tasks.as_ref(),
                    )
                    .await;
                });
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
        let pending_count = pending.read().await.len();
        let sender = {
            let mut pending_guard = pending.write().await;
            pending_guard.remove(&response.id)
        };

        if let Some(sender) = sender {
            debug!(?response.id, pending_count, "Routing response to pending request (found in pending)");
            if sender.send(response).is_err() {
                warn!("Pending request receiver dropped");
            }
        } else {
            // This can happen benignly when:
            // 1. A response arrives that was already handled (e.g., after timeout)
            // 2. The server sends an unsolicited response
            // 3. A previous response is re-delivered due to transport buffering
            // Log at debug level to help diagnose correlation issues.
            debug!(?response.id, pending_count, "Response not found in pending (possible race or duplicate)");
        }
    }

    /// Handle a server-initiated request.
    async fn handle_server_request(
        request: Request,
        handler: &Arc<H>,
        transport: &Arc<T>,
        client_caps: &Arc<ClientCapabilities>,
        tasks: Option<&Arc<TaskManager>>,
    ) {
        trace!(method = %request.method, "Handling server request");

        let response = match request.method.as_ref() {
            "sampling/createMessage" => {
                Self::handle_sampling_request(&request, handler, client_caps, tasks).await
            }
            method @ ("tasks/get" | "tasks/result" | "tasks/list" | "tasks/cancel") => {
                Self::handle_tasks_request(&request, method, client_caps, tasks).await
            }
            "elicitation/create" => Self::handle_elicitation_request(&request, handler).await,
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
    async fn handle_sampling_request(
        request: &Request,
        handler: &Arc<H>,
        client_caps: &Arc<ClientCapabilities>,
        tasks: Option<&Arc<TaskManager>>,
    ) -> Response {
        let mut params = match &request.params {
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

        // Per spec, a client MUST reject tool-augmented sampling unless it
        // declared the `sampling.tools` capability.
        if (params.tools.is_some() || params.tool_choice.is_some())
            && !client_caps.has_sampling_tools()
        {
            return Response::error(
                request.id.clone(),
                JsonRpcError::invalid_params(
                    "sampling request includes tools/toolChoice but the client did not \
                     declare the sampling.tools capability",
                ),
            );
        }

        if params.task.is_some() {
            // Task-augmented path: only when the client declared
            // `tasks.requests.sampling.createMessage`. Reply with
            // `CreateTaskResult` immediately and run the sampling in the
            // background; the store holds the outcome for the server's later
            // `tasks/get` / `tasks/result`.
            if let (true, Some(store)) = (client_caps.has_task_sampling(), tasks) {
                let ttl = params.task.as_ref().and_then(|t| t.ttl);
                let handle = store.create(ttl);
                let task = handle
                    .task()
                    .unwrap_or_else(|| mcpkit_core::types::Task::new(handle.id().clone()));
                let create_result =
                    serde_json::to_value(mcpkit_core::types::CreateTaskResult { task, meta: None })
                        .unwrap_or_default();

                // The handler never observes the task metadata; its
                // cancellation arrives through the context instead.
                params.task = None;
                let ctx =
                    RequestContext::with_cancellation(handle.cancel_token().unwrap_or_default());
                let handler = Arc::clone(handler);
                tokio::spawn(async move {
                    match handler.create_message(params, &ctx).await {
                        Ok(result) => match serde_json::to_value(result) {
                            Ok(value) => {
                                // Fails only if the task went terminal first
                                // (e.g. cancelled) -- the outcome is discarded.
                                let _ = handle.complete(value);
                            }
                            Err(e) => {
                                let _ = handle.fail(format!("Serialization error: {e}"));
                            }
                        },
                        // tasks/result must reproduce the JSON-RPC error the
                        // request would have returned.
                        Err(e) => {
                            let _ = handle.fail_with_error((&e).into());
                        }
                    }
                });
                return Response::success(request.id.clone(), create_result);
            }

            // Undeclared: per spec the request is processed normally, ignoring
            // the task metadata. Strip it so handlers cannot observe a field
            // the spec requires us to ignore.
            params.task = None;
        }

        match handler
            .create_message(params, &RequestContext::default())
            .await
        {
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

    /// Serve a server-initiated `tasks/*` request against the client's own
    /// task store (tasks created by task-augmented server->client requests).
    async fn handle_tasks_request(
        request: &Request,
        method: &str,
        client_caps: &Arc<ClientCapabilities>,
        tasks: Option<&Arc<TaskManager>>,
    ) -> Response {
        // No declared `tasks` capability -> unknown method (the pre-tasks
        // behavior).
        let Some(store) = tasks else {
            return Response::error(
                request.id.clone(),
                JsonRpcError::method_not_found(format!("Unknown method: {method}")),
            );
        };
        // `tasks/list` and `tasks/cancel` are separately declared operations;
        // `tasks/get` / `tasks/result` are implied by any `tasks` declaration.
        let declared = match method {
            "tasks/list" => client_caps.tasks.as_ref().is_some_and(|t| t.list.is_some()),
            "tasks/cancel" => client_caps
                .tasks
                .as_ref()
                .is_some_and(|t| t.cancel.is_some()),
            _ => true,
        };
        if !declared {
            return Response::error(
                request.id.clone(),
                JsonRpcError::method_not_found(format!(
                    "{method} is not declared in the client's tasks capability"
                )),
            );
        }
        match route_task_store(store, method, request.params.as_ref()).await {
            Some(Ok(value)) => Response::success(request.id.clone(), value),
            Some(Err(e)) => Response::error(request.id.clone(), (&e).into()),
            // The store does not own this id; the client has no fallback
            // handler, so an unknown taskId is invalid params (spec).
            None => Response::error(
                request.id.clone(),
                JsonRpcError::invalid_params("Unknown task"),
            ),
        }
    }

    /// Handle an elicitation/create request.
    async fn handle_elicitation_request(request: &Request, handler: &Arc<H>) -> Response {
        let Some(p) = &request.params else {
            return Response::error(
                request.id.clone(),
                JsonRpcError::invalid_params("Missing params for elicitation/create"),
            );
        };
        // Parse either form- or url-mode params (discriminated by `mode`).
        let params = match serde_json::from_value::<ElicitRequestParams>(p.clone()) {
            Ok(params) => params,
            Err(e) => {
                return Response::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params(format!("Invalid params: {e}")),
                );
            }
        };

        let outcome = match params {
            ElicitRequestParams::Form(req) => handler.elicit(req).await,
            ElicitRequestParams::Url(req) => handler.elicit_url(req).await,
        };

        match outcome {
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
                let result = mcpkit_core::types::ListRootsResult { roots, meta: None };
                match serde_json::to_value(result) {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(
                        request.id.clone(),
                        JsonRpcError::internal_error(e.to_string()),
                    ),
                }
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
                // Handle progress notifications with typed params (progressToken
                // may be a string or a number).
                if let Some(params) = notification.params {
                    match serde_json::from_value::<mcpkit_core::types::ProgressNotificationParams>(
                        params,
                    ) {
                        Ok(params) => {
                            debug!(token = %params.progress_token, "Progress update");
                            handler.on_progress(params).await;
                        }
                        Err(e) => debug!(error = %e, "Ignoring malformed progress notification"),
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
            "notifications/elicitation/complete" => {
                if let Some(id) = notification
                    .params
                    .as_ref()
                    .and_then(|p| p.get("elicitationId"))
                    .and_then(|v| v.as_str())
                {
                    debug!(elicitation_id = %id, "Elicitation completed");
                    handler.on_elicitation_complete(id.to_string()).await;
                }
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

    /// Follow `nextCursor` pagination to exhaustion, accumulating every page.
    ///
    /// `extract` pulls the page's items and its `nextCursor` out of each result.
    /// A server that hands back the same cursor it was given (i.e. makes no
    /// forward progress) is rejected rather than looped on forever.
    async fn list_all<Item, R>(
        &self,
        method: &str,
        extract: impl Fn(R) -> (Vec<Item>, Option<String>),
    ) -> Result<Vec<Item>, McpError>
    where
        R: serde::de::DeserializeOwned,
    {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor
                .as_deref()
                .map(|c| serde_json::json!({ "cursor": c }));
            let result: R = self.request(method, params).await?;
            let (items, next) = extract(result);
            all.extend(items);
            match next {
                Some(next) if cursor.as_ref() != Some(&next) => cursor = Some(next),
                Some(_) => {
                    return Err(McpError::internal(format!(
                        "{method} returned a non-advancing pagination cursor"
                    )));
                }
                None => return Ok(all),
            }
        }
    }

    /// List all available tools, following pagination to exhaustion.
    ///
    /// # Errors
    ///
    /// Returns an error if tools are not supported or the request fails.
    pub async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        self.ensure_capability("tools", self.has_tools())?;
        self.list_all("tools/list", |r: ListToolsResult| (r.tools, r.next_cursor))
            .await
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
    /// * `arguments` - The arguments to pass to the tool (as JSON). The spec
    ///   requires `arguments` to be a JSON **object**; `null` is treated as
    ///   "no arguments".
    ///
    /// # Errors
    ///
    /// Returns an error if tools are not supported, `arguments` is neither an
    /// object nor `null`, or the call fails.
    pub async fn call_tool(
        &self,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        self.ensure_capability("tools", self.has_tools())?;

        let arguments = match arguments {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            _ => {
                return Err(McpError::invalid_params(
                    "tools/call",
                    "arguments must be a JSON object",
                ));
            }
        };
        let request = CallToolRequest {
            name: name.into(),
            arguments,
            task: None,
        };
        self.request("tools/call", Some(serde_json::to_value(request)?))
            .await
    }

    // ==========================================================================
    // Resource Operations
    // ==========================================================================

    /// List all available resources, following pagination to exhaustion.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are not supported or the request fails.
    pub async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        self.ensure_capability("resources", self.has_resources())?;
        self.list_all("resources/list", |r: ListResourcesResult| {
            (r.resources, r.next_cursor)
        })
        .await
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

    /// List resource templates, following pagination to exhaustion.
    ///
    /// # Errors
    ///
    /// Returns an error if resources are not supported or the request fails.
    pub async fn list_resource_templates(&self) -> Result<Vec<ResourceTemplate>, McpError> {
        self.ensure_capability("resources", self.has_resources())?;
        self.list_all(
            "resources/templates/list",
            |r: ListResourceTemplatesResult| (r.resource_templates, r.next_cursor),
        )
        .await
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

    /// List all available prompts, following pagination to exhaustion.
    ///
    /// # Errors
    ///
    /// Returns an error if prompts are not supported or the request fails.
    pub async fn list_prompts(&self) -> Result<Vec<Prompt>, McpError> {
        self.ensure_capability("prompts", self.has_prompts())?;
        self.list_all("prompts/list", |r: ListPromptsResult| {
            (r.prompts, r.next_cursor)
        })
        .await
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
    pub async fn list_tasks(&self) -> Result<Vec<Task>, McpError> {
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

        // `tasks/list` has no server-side status filter in the spec; filter the
        // returned page client-side when a status is requested.
        let request = ListTasksRequest {
            cursor: cursor.map(String::from),
        };
        let mut result: ListTasksResult = self
            .request("tasks/list", Some(serde_json::to_value(request)?))
            .await?;
        if let Some(status) = status {
            result.tasks.retain(|task| task.status == status);
        }
        Ok(result)
    }

    /// Get a task by ID.
    ///
    /// Returns the [`GetTaskResult`] wrapper (`Result & Task`), preserving any
    /// result-level `_meta`; the task state is in its `task` field.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks are not supported or the task is not found.
    pub async fn get_task(&self, id: impl Into<String>) -> Result<GetTaskResult, McpError> {
        self.ensure_capability("tasks", self.has_tasks())?;

        let request = GetTaskRequest {
            task_id: id.into().into(),
        };
        self.request("tasks/get", Some(serde_json::to_value(request)?))
            .await
    }

    /// Cancel a running task, returning its post-cancellation state.
    ///
    /// Returns the [`CancelTaskResult`] wrapper (`Result & Task`), preserving any
    /// result-level `_meta`; the task state is in its `task` field.
    ///
    /// # Errors
    ///
    /// Returns an error if tasks are not supported, cancellation is not supported,
    /// or the task is not found.
    pub async fn cancel_task(&self, id: impl Into<String>) -> Result<CancelTaskResult, McpError> {
        self.ensure_capability("tasks", self.has_tasks())?;

        let request = CancelTaskRequest {
            task_id: id.into().into(),
        };
        self.request("tasks/cancel", Some(serde_json::to_value(request)?))
            .await
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
            context: None,
        };
        self.complete(request).await
    }

    /// Send a completion request, including any previously-resolved
    /// [`context`](CompleteRequest::context).
    ///
    /// The `complete_prompt_argument`/`complete_resource_argument` helpers cover
    /// the common no-context case; use this for URI-template or multi-argument
    /// completion where earlier values must be supplied.
    ///
    /// # Errors
    ///
    /// Returns an error if completions are not supported or the request fails.
    pub async fn complete(&self, request: CompleteRequest) -> Result<CompleteResult, McpError> {
        self.ensure_capability("completions", self.has_completions())?;
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
            context: None,
        };
        self.complete(request).await
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

        let request = SubscribeRequest { uri: uri.into() };
        let _: serde_json::Value = self
            .request("resources/subscribe", Some(serde_json::to_value(request)?))
            .await?;
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

        let request = UnsubscribeRequest { uri: uri.into() };
        let _: serde_json::Value = self
            .request(
                "resources/unsubscribe",
                Some(serde_json::to_value(request)?),
            )
            .await?;
        Ok(())
    }

    /// Notify the server that this client's root list has changed
    /// (`notifications/roots/list_changed`, sent with no params).
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be queued for sending.
    pub async fn notify_roots_list_changed(&self) -> Result<(), McpError> {
        self.outgoing_tx
            .send(Message::Notification(Notification::new(
                "notifications/roots/list_changed",
            )))
            .await
            .map_err(|_| {
                McpError::Transport(Box::new(TransportDetails {
                    kind: TransportErrorKind::WriteFailed,
                    message: "Failed to send roots/list_changed (channel closed)".to_string(),
                    context: TransportContext::default(),
                    source: None,
                }))
            })
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

    /// Send a request with a `progressToken` attached (via `_meta.progressToken`)
    /// so the server may emit `notifications/progress` for the call. Progress
    /// updates are delivered to the client handler's
    /// [`on_progress`](crate::ClientHandler::on_progress).
    ///
    /// `params` is the method's normal params object (or `None`); the token is
    /// merged into its `_meta`.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or times out.
    pub async fn request_with_progress<R: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
        token: mcpkit_core::protocol::ProgressToken,
    ) -> Result<R, McpError> {
        let params = mcpkit_core::types::Meta::with_progress_token_in_params(params, &token);
        self.request(method, Some(params)).await
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

        // Wait for the response, bounded by the configured request timeout.
        // On either elapse or a dropped sender we must remove our entry from
        // `pending`, otherwise stale senders accumulate without bound.
        let response = match tokio::time::timeout(self.request_timeout, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                // Sender was dropped (router exited / connection closed).
                self.pending.write().await.remove(&id);
                return Err(McpError::Transport(Box::new(TransportDetails {
                    kind: TransportErrorKind::ConnectionClosed,
                    message: "Response channel closed (server may have disconnected)".to_string(),
                    context: TransportContext::default(),
                    source: None,
                })));
            }
            Err(_elapsed) => {
                self.pending.write().await.remove(&id);
                return Err(McpError::Transport(Box::new(TransportDetails {
                    kind: TransportErrorKind::Timeout,
                    message: format!(
                        "Request '{method}' timed out after {:?}",
                        self.request_timeout
                    ),
                    context: TransportContext::default(),
                    source: None,
                })));
            }
        };

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
    use mcpkit_transport::TransportMetadata;

    #[test]
    fn test_request_id_generation() {
        let next_id = AtomicU64::new(1);
        assert_eq!(next_id.fetch_add(1, Ordering::SeqCst), 1);
        assert_eq!(next_id.fetch_add(1, Ordering::SeqCst), 2);
    }

    fn test_init_result() -> InitializeResult {
        InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities::new(),
            server_info: ServerInfo::new("test-server", "1.0.0"),
            instructions: None,
            meta: None,
        }
    }

    /// A transport that accepts sends but never delivers a response.
    struct SilentTransport;

    impl Transport for SilentTransport {
        type Error = std::convert::Infallible;

        async fn send(&self, _msg: Message) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn recv(&self) -> Result<Option<Message>, Self::Error> {
            std::future::pending().await
        }

        async fn close(&self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn metadata(&self) -> TransportMetadata {
            TransportMetadata::new("silent-test")
        }
    }

    /// A transport that reports a clean close (`recv` -> `Ok(None)`) as soon as
    /// the first message is sent, simulating a server that disconnects while a
    /// request is in flight.
    struct ClosingTransport {
        on_send: Arc<tokio::sync::Notify>,
    }

    impl ClosingTransport {
        fn new() -> Self {
            Self {
                on_send: Arc::new(tokio::sync::Notify::new()),
            }
        }
    }

    impl Transport for ClosingTransport {
        type Error = std::convert::Infallible;

        async fn send(&self, _msg: Message) -> Result<(), Self::Error> {
            self.on_send.notify_one();
            Ok(())
        }

        async fn recv(&self) -> Result<Option<Message>, Self::Error> {
            self.on_send.notified().await;
            Ok(None)
        }

        async fn close(&self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn metadata(&self) -> TransportMetadata {
            TransportMetadata::new("closing-test")
        }
    }

    /// Regression test for #5: a request to a server that never responds must
    /// fail with a timeout (not hang forever) and must remove its entry from the
    /// pending map so it cannot accumulate without bound.
    #[tokio::test(start_paused = true)]
    async fn request_times_out_and_drains_pending() {
        let client = Client::new(
            SilentTransport,
            test_init_result(),
            ClientInfo::new("test-client", "1.0.0"),
            ClientCapabilities::default(),
            Duration::from_secs(5),
        );

        let err = client
            .request::<serde_json::Value>("tools/list", None)
            .await
            .expect_err("request should time out");

        match err {
            McpError::Transport(details) => {
                assert!(
                    matches!(details.kind, TransportErrorKind::Timeout),
                    "expected Timeout, got {:?}",
                    details.kind
                );
            }
            other => panic!("expected transport timeout, got {other:?}"),
        }

        assert!(
            client.pending.read().await.is_empty(),
            "timed-out request must be removed from the pending map"
        );
    }

    /// Regression test for #5: when the connection closes while a request is in
    /// flight, the request must fail fast rather than wait out the timeout. The
    /// generous timeout below means a regression of the drain would hang the test.
    #[tokio::test]
    async fn in_flight_request_fails_fast_when_connection_closes() {
        let client = Client::new(
            ClosingTransport::new(),
            test_init_result(),
            ClientInfo::new("test-client", "1.0.0"),
            ClientCapabilities::default(),
            Duration::from_secs(3600),
        );

        let err = client
            .request::<serde_json::Value>("tools/list", None)
            .await
            .expect_err("request should fail when the connection closes");

        match err {
            McpError::Transport(details) => {
                assert!(
                    matches!(details.kind, TransportErrorKind::ConnectionClosed),
                    "expected ConnectionClosed, got {:?}",
                    details.kind
                );
            }
            other => panic!("expected transport connection-closed, got {other:?}"),
        }

        assert!(
            client.pending.read().await.is_empty(),
            "pending requests must be drained when the connection closes"
        );
    }

    #[tokio::test]
    async fn call_tool_rejects_non_object_arguments_before_sending() {
        let init = InitializeResult {
            capabilities: ServerCapabilities::new().with_tools(),
            ..test_init_result()
        };
        let client = Client::new(
            SilentTransport,
            init,
            ClientInfo::new("test-client", "1.0.0"),
            ClientCapabilities::default(),
            Duration::from_secs(3600),
        );

        // Rejected locally, so this returns immediately even though the
        // transport never answers.
        let err = client
            .call_tool("add", serde_json::json!(5))
            .await
            .expect_err("non-object arguments must be rejected");
        assert!(
            err.to_string().contains("arguments must be a JSON object"),
            "expected invalid-params on arguments, got: {err}"
        );
    }

    /// A transport that serves `tools/list` in fixed-size pages, echoing an
    /// opaque numeric `nextCursor`. With `stuck_cursor` it always returns the
    /// same cursor, to exercise the non-advancing-cursor guard.
    struct PaginatingTransport {
        resp_tx: tokio::sync::mpsc::UnboundedSender<Message>,
        resp_rx: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Message>>,
        total: usize,
        page_size: usize,
        stuck_cursor: bool,
    }

    impl PaginatingTransport {
        fn new(total: usize, page_size: usize, stuck_cursor: bool) -> Self {
            let (resp_tx, resp_rx) = tokio::sync::mpsc::unbounded_channel();
            Self {
                resp_tx,
                resp_rx: tokio::sync::Mutex::new(resp_rx),
                total,
                page_size,
                stuck_cursor,
            }
        }
    }

    impl Transport for PaginatingTransport {
        type Error = std::convert::Infallible;

        async fn send(&self, msg: Message) -> Result<(), Self::Error> {
            let Message::Request(req) = msg else {
                return Ok(());
            };
            let offset: usize = req
                .params
                .as_ref()
                .and_then(|p| p.get("cursor"))
                .and_then(serde_json::Value::as_str)
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let end = (offset + self.page_size).min(self.total);
            let tools: Vec<serde_json::Value> = (offset..end)
                .map(|i| serde_json::json!({ "name": format!("t{i}"), "inputSchema": {} }))
                .collect();
            let mut result = serde_json::json!({ "tools": tools });
            let next = if self.stuck_cursor {
                Some("0".to_string())
            } else if end < self.total {
                Some(end.to_string())
            } else {
                None
            };
            if let Some(next) = next {
                result["nextCursor"] = serde_json::Value::String(next);
            }
            let _ = self
                .resp_tx
                .send(Message::Response(Response::success(req.id, result)));
            Ok(())
        }

        async fn recv(&self) -> Result<Option<Message>, Self::Error> {
            Ok(self.resp_rx.lock().await.recv().await)
        }

        async fn close(&self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            true
        }

        fn metadata(&self) -> TransportMetadata {
            TransportMetadata::new("paginating-test")
        }
    }

    fn tools_init_result() -> InitializeResult {
        InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities::new().with_tools(),
            server_info: ServerInfo::new("test-server", "1.0.0"),
            instructions: None,
            meta: None,
        }
    }

    /// `list_tools` must follow `nextCursor` across every page rather than
    /// silently truncating to the first page.
    #[tokio::test]
    async fn list_tools_follows_cursor_to_exhaustion() {
        let client = Client::new(
            PaginatingTransport::new(5, 2, false),
            tools_init_result(),
            ClientInfo::new("test-client", "1.0.0"),
            ClientCapabilities::default(),
            Duration::from_secs(5),
        );

        let tools = client
            .list_tools()
            .await
            .expect("list_tools should paginate");
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["t0", "t1", "t2", "t3", "t4"]);
    }

    /// A server that keeps handing back the same cursor must surface an error
    /// instead of looping forever.
    #[tokio::test]
    async fn list_tools_rejects_non_advancing_cursor() {
        let client = Client::new(
            PaginatingTransport::new(5, 2, true),
            tools_init_result(),
            ClientInfo::new("test-client", "1.0.0"),
            ClientCapabilities::default(),
            Duration::from_secs(5),
        );

        let err = client
            .list_tools()
            .await
            .expect_err("a non-advancing cursor must not loop forever");
        assert!(
            err.to_string().contains("non-advancing"),
            "expected a non-advancing-cursor error, got {err:?}"
        );
    }

    /// Regression: a `notifications/progress` with a **numeric** progress token
    /// must reach `on_progress` with typed params (the old code only accepted
    /// string tokens and mis-parsed `progress` as a `TaskProgress`).
    #[tokio::test]
    async fn progress_notification_routes_to_on_progress_with_numeric_token() {
        use mcpkit_core::protocol::ProgressToken;
        use mcpkit_core::types::ProgressNotificationParams;
        use std::sync::Mutex;

        struct Rec(Arc<Mutex<Vec<ProgressNotificationParams>>>);
        impl ClientHandler for Rec {
            async fn on_progress(&self, params: ProgressNotificationParams) {
                self.0.lock().unwrap().push(params);
            }
        }

        let seen = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(Rec(Arc::clone(&seen)));
        let notif = Notification::with_params(
            "notifications/progress",
            serde_json::json!({ "progressToken": 5, "progress": 0.25, "total": 1.0 }),
        );

        Client::<SilentTransport, Rec>::handle_notification(notif, &handler).await;

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].progress_token, ProgressToken::Number(5));
        assert!((seen[0].progress - 0.25).abs() < f64::EPSILON);
        assert_eq!(seen[0].total, Some(1.0));
    }

    /// Per spec, a client must reject tool-augmented sampling unless it declared
    /// the `sampling.tools` capability.
    #[tokio::test]
    async fn sampling_tools_requires_declared_capability() {
        let handler = Arc::new(crate::handler::NoOpHandler);
        let request = Request::with_params(
            "sampling/createMessage",
            RequestId::Number(1),
            serde_json::json!({
                "messages": [],
                "maxTokens": 10,
                "tools": [],
                "toolChoice": { "mode": "auto" }
            }),
        );

        // Declared only `sampling` (not `sampling.tools`) -> gate rejects.
        let caps = Arc::new(ClientCapabilities::new().with_sampling());
        let resp = Client::<SilentTransport, crate::handler::NoOpHandler>::handle_sampling_request(
            &request, &handler, &caps, None,
        )
        .await;
        assert!(
            resp.error
                .expect("error")
                .message
                .contains("sampling.tools"),
            "tools without sampling.tools must be rejected"
        );

        // Declared `sampling.tools` -> gate passes (NoOpHandler then declines
        // with a different error).
        let caps = Arc::new(ClientCapabilities::new().with_sampling_tools());
        let resp = Client::<SilentTransport, crate::handler::NoOpHandler>::handle_sampling_request(
            &request, &handler, &caps, None,
        )
        .await;
        assert!(
            !resp
                .error
                .expect("error")
                .message
                .contains("sampling.tools"),
            "gate should pass once sampling.tools is declared"
        );
    }

    /// Per spec, a receiver that has not declared task support for a request
    /// type must process it normally, ignoring the task metadata — the handler
    /// must never observe `task`.
    #[tokio::test]
    async fn undeclared_task_augmented_sampling_is_processed_normally() {
        struct TaskSpy {
            seen: Arc<std::sync::Mutex<Option<CreateMessageRequest>>>,
        }
        impl crate::handler::ClientHandler for TaskSpy {
            async fn create_message(
                &self,
                request: CreateMessageRequest,
                _ctx: &RequestContext,
            ) -> Result<mcpkit_core::types::CreateMessageResult, McpError> {
                *self.seen.lock().unwrap() = Some(request);
                Ok(mcpkit_core::types::CreateMessageResult {
                    role: mcpkit_core::types::Role::Assistant,
                    content: mcpkit_core::types::OneOrMany::One(
                        mcpkit_core::types::SamplingContent::Text(
                            mcpkit_core::types::TextContent {
                                text: "ok".into(),
                                annotations: None,
                                meta: None,
                            },
                        ),
                    ),
                    model: "m".into(),
                    stop_reason: None,
                    meta: None,
                })
            }
        }

        let seen = Arc::new(std::sync::Mutex::new(None));
        let handler = Arc::new(TaskSpy { seen: seen.clone() });
        let request = Request::with_params(
            "sampling/createMessage",
            RequestId::Number(1),
            serde_json::json!({
                "messages": [],
                "maxTokens": 10,
                "task": { "ttl": 60000 }
            }),
        );
        let caps = Arc::new(ClientCapabilities::new().with_sampling());

        let resp = Client::<SilentTransport, TaskSpy>::handle_sampling_request(
            &request, &handler, &caps, None,
        )
        .await;

        // Processed normally: a plain CreateMessageResult, not a task.
        let result = resp.result.expect("normal result");
        assert!(result.get("task").is_none());
        assert_eq!(result["role"], "assistant");
        // The handler observed the request with `task` stripped.
        let seen = seen.lock().unwrap();
        let request_seen = seen.as_ref().expect("handler was invoked");
        assert!(
            request_seen.task.is_none(),
            "handler must not observe the task field"
        );
    }

    // --- #143 phase 3: task-augmented sampling ------------------------------

    fn assistant_result() -> mcpkit_core::types::CreateMessageResult {
        mcpkit_core::types::CreateMessageResult {
            role: mcpkit_core::types::Role::Assistant,
            content: mcpkit_core::types::OneOrMany::One(mcpkit_core::types::SamplingContent::Text(
                mcpkit_core::types::TextContent {
                    text: "ok".into(),
                    annotations: None,
                    meta: None,
                },
            )),
            model: "m".into(),
            stop_reason: None,
            meta: None,
        }
    }

    fn task_caps() -> Arc<ClientCapabilities> {
        Arc::new(
            ClientCapabilities::new()
                .with_sampling()
                .with_tasks()
                .with_task_sampling(),
        )
    }

    fn sampling_request_with_task() -> Request {
        Request::with_params(
            "sampling/createMessage",
            RequestId::Number(1),
            serde_json::json!({
                "messages": [],
                "maxTokens": 10,
                "task": { "ttl": 60000 }
            }),
        )
    }

    fn tasks_request(id: u64, method: &'static str, task_id: &str) -> Request {
        Request::with_params(
            method,
            RequestId::Number(id),
            serde_json::json!({ "taskId": task_id }),
        )
    }

    /// A handler whose sampling completes only when released; records whether
    /// it observed the task field and whether its context was cancelled.
    struct GatedSampler {
        release: Arc<tokio::sync::Notify>,
        saw_task: Arc<std::sync::Mutex<Option<bool>>>,
        cancelled: Arc<AtomicBool>,
    }
    impl crate::handler::ClientHandler for GatedSampler {
        async fn create_message(
            &self,
            request: CreateMessageRequest,
            ctx: &RequestContext,
        ) -> Result<mcpkit_core::types::CreateMessageResult, McpError> {
            *self.saw_task.lock().unwrap() = Some(request.task.is_some());
            tokio::select! {
                () = self.release.notified() => Ok(assistant_result()),
                () = ctx.cancelled() => {
                    self.cancelled.store(true, Ordering::SeqCst);
                    Err(McpError::cancelled("sampling"))
                }
            }
        }
    }

    #[tokio::test]
    async fn task_augmented_sampling_returns_task_then_result() {
        type C = Client<SilentTransport, GatedSampler>;
        let release = Arc::new(tokio::sync::Notify::new());
        let saw_task = Arc::new(std::sync::Mutex::new(None));
        let handler = Arc::new(GatedSampler {
            release: release.clone(),
            saw_task: saw_task.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let caps = task_caps();
        let store = Arc::new(TaskManager::new());

        // Task-augmented request -> immediate CreateTaskResult, status working.
        let resp = C::handle_sampling_request(
            &sampling_request_with_task(),
            &handler,
            &caps,
            Some(&store),
        )
        .await;
        let result = resp.result.expect("CreateTaskResult");
        assert_eq!(result["task"]["status"], "working");
        assert_eq!(result["task"]["ttl"], 60000);
        let task_id = result["task"]["taskId"].as_str().expect("id").to_string();

        // tasks/get while the sampling is still gated: working.
        let got = C::handle_tasks_request(
            &tasks_request(2, "tasks/get", &task_id),
            "tasks/get",
            &caps,
            Some(&store),
        )
        .await;
        assert_eq!(got.result.expect("task")["status"], "working");

        // tasks/result blocks until the handler is released, then returns the
        // CreateMessageResult with the related-task metadata attached.
        let waiter = {
            let caps = Arc::clone(&caps);
            let store = Arc::clone(&store);
            let task_id = task_id.clone();
            tokio::spawn(async move {
                C::handle_tasks_request(
                    &tasks_request(3, "tasks/result", &task_id),
                    "tasks/result",
                    &caps,
                    Some(&store),
                )
                .await
            })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !waiter.is_finished(),
            "tasks/result must block until terminal"
        );
        release.notify_one();
        let resp = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .expect("blocked tasks/result never completed")
            .expect("join");
        let payload = resp.result.expect("sampling result");
        assert_eq!(payload["role"], "assistant");
        assert_eq!(
            payload["_meta"]["io.modelcontextprotocol/related-task"]["taskId"],
            task_id.as_str()
        );
        // The handler never observed the task field.
        assert_eq!(*saw_task.lock().unwrap(), Some(false));
    }

    #[tokio::test]
    async fn tasks_cancel_cancels_in_flight_sampling() {
        type C = Client<SilentTransport, GatedSampler>;
        let cancelled = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(GatedSampler {
            release: Arc::new(tokio::sync::Notify::new()),
            saw_task: Arc::new(std::sync::Mutex::new(None)),
            cancelled: cancelled.clone(),
        });
        let caps = task_caps();
        let store = Arc::new(TaskManager::new());

        let resp = C::handle_sampling_request(
            &sampling_request_with_task(),
            &handler,
            &caps,
            Some(&store),
        )
        .await;
        let task_id = resp.result.expect("CreateTaskResult")["task"]["taskId"]
            .as_str()
            .expect("id")
            .to_string();

        // Cancel while the sampling is in flight.
        let resp = C::handle_tasks_request(
            &tasks_request(2, "tasks/cancel", &task_id),
            "tasks/cancel",
            &caps,
            Some(&store),
        )
        .await;
        assert_eq!(resp.result.expect("cancel result")["status"], "cancelled");

        // The handler's context observed the cancellation.
        for _ in 0..100 {
            if cancelled.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            cancelled.load(Ordering::SeqCst),
            "handler must observe cancellation via its RequestContext"
        );

        // The task stays cancelled; tasks/result reports no result.
        let resp = C::handle_tasks_request(
            &tasks_request(3, "tasks/result", &task_id),
            "tasks/result",
            &caps,
            Some(&store),
        )
        .await;
        let err = resp.error.expect("cancelled task has no result");
        assert_eq!(err.code, -32602);
        // Status remains cancelled even though the handler returned an error
        // after cancellation.
        let got = C::handle_tasks_request(
            &tasks_request(4, "tasks/get", &task_id),
            "tasks/get",
            &caps,
            Some(&store),
        )
        .await;
        assert_eq!(got.result.expect("task")["status"], "cancelled");
    }

    #[tokio::test]
    async fn tasks_list_and_cancel_are_capability_gated() {
        type C = Client<SilentTransport, crate::handler::NoOpHandler>;
        // Declared task sampling but NOT tasks.list / tasks.cancel.
        let caps = Arc::new(ClientCapabilities::new().with_task_sampling());
        let store = Arc::new(TaskManager::new());

        let resp = C::handle_tasks_request(
            &Request::with_params("tasks/list", RequestId::Number(1), serde_json::json!({})),
            "tasks/list",
            &caps,
            Some(&store),
        )
        .await;
        assert_eq!(resp.error.expect("gated").code, -32601);

        let resp = C::handle_tasks_request(
            &tasks_request(2, "tasks/cancel", "nope"),
            "tasks/cancel",
            &caps,
            Some(&store),
        )
        .await;
        assert_eq!(resp.error.expect("gated").code, -32601);

        // tasks/get is implied by any tasks declaration; unknown id -> -32602.
        let resp = C::handle_tasks_request(
            &tasks_request(3, "tasks/get", "nope"),
            "tasks/get",
            &caps,
            Some(&store),
        )
        .await;
        assert_eq!(resp.error.expect("unknown task").code, -32602);
    }
}
