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

use crate::builder::Server;
use crate::context::{CancellationToken, Context, ContextData, Peer};
use crate::dispatch::{PromptSlot, ResourceSlot, TaskSlot, ToolSlot};
use crate::handler::ServerHandler;
use crate::router::{route_prompts, route_resources, route_tasks, route_tools};
use futures::channel::oneshot;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::{Message, Notification, ProgressToken, Request, RequestId, Response};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_transport::Transport;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

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
    /// Response channels for in-flight server-initiated (outbound) requests,
    /// keyed by the outbound request id.
    pending_requests: RwLock<HashMap<RequestId, oneshot::Sender<Response>>>,
    /// Monotonic counter for allocating outbound request ids.
    outbound_id: AtomicU64,
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
            pending_requests: RwLock::new(HashMap::new()),
            outbound_id: AtomicU64::new(1),
        }
    }

    /// Allocate a unique id for a server-initiated (outbound) request.
    pub(crate) fn next_outbound_id(&self) -> RequestId {
        RequestId::Number(self.outbound_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Register a pending outbound request, returning the receiver that resolves
    /// when the matching response arrives.
    pub(crate) fn register_outbound(&self, id: RequestId) -> oneshot::Receiver<Response> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending_requests.write() {
            pending.insert(id, tx);
        }
        rx
    }

    /// Drop a pending outbound request (e.g. on timeout or cancellation).
    pub(crate) fn remove_outbound(&self, id: &RequestId) {
        if let Ok(mut pending) = self.pending_requests.write() {
            pending.remove(id);
        }
    }

    /// Route an inbound response to the outbound request that is waiting for it.
    pub(crate) fn route_response(&self, response: Response) {
        let sender = self
            .pending_requests
            .write()
            .ok()
            .and_then(|mut pending| pending.remove(&response.id));
        match sender {
            Some(sender) => {
                let _ = sender.send(response);
            }
            None => {
                tracing::debug!(id = %response.id, "response did not match a pending request");
            }
        }
    }

    /// Fail every pending outbound request (e.g. the connection closed). Dropping
    /// the senders makes the waiting receivers resolve with an error.
    pub(crate) fn fail_pending_requests(&self) {
        if let Ok(mut pending) = self.pending_requests.write() {
            pending.clear();
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

/// Shared state a [`TransportPeer`] needs to make server-initiated requests:
/// the pending-request registry (on [`ServerState`]) and the outbound timeout.
#[derive(Clone)]
struct OutboundCtx {
    state: Arc<ServerState>,
    timeout: Duration,
}

/// A peer implementation that sends notifications over a transport.
///
/// Constructed with [`new`](Self::new) it can only send notifications. The
/// runtime builds request-capable peers (with a pending-request registry) for
/// handler contexts via `with_outbound`.
pub struct TransportPeer<T: Transport> {
    transport: Arc<T>,
    outbound: Option<OutboundCtx>,
}

impl<T: Transport> TransportPeer<T> {
    /// Create a new notification-only transport peer.
    pub const fn new(transport: Arc<T>) -> Self {
        Self {
            transport,
            outbound: None,
        }
    }

    /// Create a request-capable transport peer that correlates responses through
    /// the given server state.
    pub(crate) fn with_outbound(
        transport: Arc<T>,
        state: Arc<ServerState>,
        timeout: Duration,
    ) -> Self {
        Self {
            transport,
            outbound: Some(OutboundCtx { state, timeout }),
        }
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

    fn request(
        &self,
        method: std::borrow::Cow<'static, str>,
        params: Option<serde_json::Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, McpError>> + Send + '_>>
    {
        let Some(outbound) = self.outbound.clone() else {
            return Box::pin(async {
                Err(McpError::internal(
                    "this peer does not support server-initiated requests",
                ))
            });
        };
        let transport = self.transport.clone();
        Box::pin(async move {
            use futures::future::{Either, select};

            let id = outbound.state.next_outbound_id();
            let rx = outbound.state.register_outbound(id.clone());
            let request = match params {
                Some(p) => Request::with_params(method, id.clone(), p),
                None => Request::new(method, id.clone()),
            };
            transport
                .send(Message::Request(request))
                .await
                .map_err(std::convert::Into::into)?;

            let sleep = mcpkit_transport::runtime::sleep(outbound.timeout);
            futures::pin_mut!(sleep);
            match select(rx, sleep).await {
                Either::Left((Ok(response), _)) => Ok(response),
                Either::Left((Err(_canceled), _)) => {
                    outbound.state.remove_outbound(&id);
                    Err(McpError::internal(
                        "response channel closed before a reply arrived",
                    ))
                }
                Either::Right(((), _)) => {
                    outbound.state.remove_outbound(&id);
                    Err(McpError::internal(format!(
                        "server-initiated request timed out after {:?}",
                        outbound.timeout
                    )))
                }
            }
        })
    }
}

/// A cloneable handle for sending server-initiated notifications from outside a
/// request context.
///
/// A handler's [`Context`] can only send notifications while a request is being
/// served. When the server's own state changes between requests — for example
/// its tool set changes — use a `ServerNotifier` to push the corresponding
/// notification (`tools/list_changed`, `resources/list_changed`, etc.) to the
/// client.
///
/// Obtain one from [`ServerRuntime::notifier`] before spawning the runtime:
///
/// ```rust,ignore
/// let runtime = ServerRuntime::new(server, transport);
/// let notifier = runtime.notifier();
/// tokio::spawn(async move { runtime.run().await });
///
/// // later, from anywhere:
/// notifier.tools_list_changed().await?;
/// ```
#[derive(Clone)]
pub struct ServerNotifier {
    peer: Arc<dyn Peer>,
}

impl ServerNotifier {
    /// Send a notification with the given method and optional params.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent over the transport.
    pub async fn notify(
        &self,
        method: impl Into<std::borrow::Cow<'static, str>>,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpError> {
        let notification = match params {
            Some(p) => Notification::with_params(method, p),
            None => Notification::new(method),
        };
        self.peer.notify(notification).await
    }

    /// Notify the client that the available tool list has changed.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn tools_list_changed(&self) -> Result<(), McpError> {
        self.notify(crate::router::notifications::TOOLS_LIST_CHANGED, None)
            .await
    }

    /// Notify the client that the available resource list has changed.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn resources_list_changed(&self) -> Result<(), McpError> {
        self.notify(crate::router::notifications::RESOURCES_LIST_CHANGED, None)
            .await
    }

    /// Notify the client that the available prompt list has changed.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn prompts_list_changed(&self) -> Result<(), McpError> {
        self.notify(crate::router::notifications::PROMPTS_LIST_CHANGED, None)
            .await
    }

    /// Notify the client that a subscribed resource was updated.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn resource_updated(&self, uri: impl Into<String>) -> Result<(), McpError> {
        self.notify(
            crate::router::notifications::RESOURCES_UPDATED,
            Some(serde_json::json!({ "uri": uri.into() })),
        )
        .await
    }

    /// Notify the client that a URL-mode elicitation's out-of-band interaction
    /// has completed (`notifications/elicitation/complete`).
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn elicitation_complete(
        &self,
        elicitation_id: impl Into<String>,
    ) -> Result<(), McpError> {
        self.notify(
            crate::router::notifications::ELICITATION_COMPLETE,
            Some(serde_json::json!({ "elicitationId": elicitation_id.into() })),
        )
        .await
    }
}

/// Server runtime configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Whether to automatically send initialized notification.
    pub auto_initialized: bool,
    /// Maximum concurrent requests to process.
    pub max_concurrent_requests: usize,
    /// How long a server-initiated request (e.g. elicitation, sampling) waits
    /// for the client's response before failing.
    pub outbound_request_timeout: Duration,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            auto_initialized: true,
            max_concurrent_requests: 100,
            outbound_request_timeout: Duration::from_secs(60),
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
    /// Built-in store for task-augmented execution (the runtime creates tasks
    /// here and serves `tasks/*` from it).
    task_store: Arc<crate::capability::tasks::TaskManager>,
    /// Runtime configuration (concurrency limit, etc.).
    config: RuntimeConfig,
}

/// A task-augmented `tools/call` whose tool runs in the background after the
/// `CreateTaskResult` reply has been sent.
struct BackgroundExec {
    handle: crate::capability::tasks::TaskHandle,
    name: String,
    args: serde_json::Value,
    ctx_data: ContextData,
    cancel: CancellationToken,
}

/// Outcome of inspecting a request for task augmentation.
enum TaskBegin {
    /// Not a task-augmented `tools/call`; handle it normally.
    NotApplicable,
    /// The augmentation was rejected and an error response was already sent.
    Rejected,
    /// A task was created and `CreateTaskResult` sent; run this in the background.
    Deferred(Box<BackgroundExec>),
}

/// Make progress on in-flight requests and background task executions, returning
/// any new background work an in-flight request just produced. Background tasks
/// are polled here but do not count against the request concurrency limit.
async fn drive_sets<F1, F2>(
    in_flight: &mut futures::stream::FuturesUnordered<F1>,
    background: &mut futures::stream::FuturesUnordered<F2>,
) -> Option<BackgroundExec>
where
    F1: std::future::Future<Output = Option<BackgroundExec>>,
    F2: std::future::Future<Output = ()>,
{
    use futures::future::{Either, select};
    use futures::stream::StreamExt;
    if in_flight.is_empty() {
        background.next().await;
        return None;
    }
    if background.is_empty() {
        return in_flight.next().await.flatten();
    }
    match select(in_flight.next(), background.next()).await {
        Either::Left((res, _)) => res.flatten(),
        Either::Right((_finished, _)) => None,
    }
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

    /// Get a cloneable [`ServerNotifier`] for sending server-initiated
    /// notifications (e.g. `tools/list_changed`) from outside a request context.
    ///
    /// Call this before spawning [`run`](Self::run); the returned handle shares
    /// the runtime's transport and can be used from any task.
    #[must_use]
    pub fn notifier(&self) -> ServerNotifier {
        ServerNotifier {
            peer: Arc::new(TransportPeer::new(self.transport.clone())),
        }
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

        // What the loop should do next, decided after borrows on the future sets
        // are released so we can push new background work.
        enum Step {
            Message(Option<Message>),
            Progress(Option<BackgroundExec>),
        }

        let max = self.config.max_concurrent_requests.max(1);
        let mut in_flight = FuturesUnordered::new();
        // Task-augmented tool executions run here, off the request concurrency
        // limit, so long-running tasks never starve normal request handling.
        let mut background = FuturesUnordered::new();
        // Requests received while at the concurrency limit. They run as soon as a
        // slot frees. Crucially the loop keeps receiving in the meantime, so a
        // handler parked on its own server-initiated request (which needs an
        // inbound response to complete) cannot deadlock the loop.
        let mut queued: std::collections::VecDeque<Request> = std::collections::VecDeque::new();

        let outcome = loop {
            // Dispatch queued requests while concurrency slots are free.
            while in_flight.len() < max {
                let Some(request) = queued.pop_front() else {
                    break;
                };
                in_flight.push(self.handle_request_isolated(request));
            }

            // Always receive (so responses to our own outbound requests are
            // routed even when every slot is parked) while making progress on
            // in-flight requests and background tasks.
            let step = if in_flight.is_empty() && background.is_empty() {
                match self.transport.recv().await {
                    Ok(opt) => Step::Message(opt),
                    Err(e) => break Err(e.into()),
                }
            } else {
                let recv = std::pin::pin!(self.transport.recv());
                let progress = std::pin::pin!(drive_sets(&mut in_flight, &mut background));
                match select(recv, progress).await {
                    Either::Left((Ok(opt), _)) => Step::Message(opt),
                    Either::Left((Err(e), _)) => break Err(e.into()),
                    Either::Right((maybe_exec, _)) => Step::Progress(maybe_exec),
                }
            };

            // Borrows on the future sets are released here, so we may push work.
            match step {
                Step::Progress(Some(exec)) => {
                    background.push(self.run_task(exec));
                }
                Step::Progress(None) => {}
                Step::Message(Some(Message::Request(request))) => {
                    if in_flight.len() < max {
                        in_flight.push(self.handle_request_isolated(request));
                    } else {
                        queued.push_back(request);
                    }
                }
                Step::Message(Some(Message::Notification(notification))) => {
                    if let Err(e) = self.handle_notification(notification).await {
                        tracing::error!(error = %e, "Error handling notification");
                    }
                }
                Step::Message(Some(Message::Response(response))) => {
                    // A reply to a server-initiated request (elicitation, etc.).
                    self.state.route_response(response);
                }
                Step::Message(None) => {
                    tracing::info!("Connection closed");
                    break Ok(());
                }
            }
        };

        // The connection is going away: fail any in-flight outbound requests so
        // handlers parked on them unblock, then drain the handlers so their
        // responses are delivered before we return. Background tasks are drained
        // too so their results are stored before we exit.
        self.state.fail_pending_requests();
        while in_flight.next().await.is_some() {}
        while background.next().await.is_some() {}

        if let Err(ref err) = outcome {
            tracing::error!(error = %err, "Transport error");
        }
        outcome
    }

    /// Compute the result for a request without sending it.
    async fn compute_response(&self, request: &Request) -> Result<serde_json::Value, McpError> {
        match request.method.as_ref() {
            "initialize" => self.handle_initialize(request).await,
            // `ping` is a liveness check and is valid at any time, including
            // before the initialize handshake completes.
            "ping" => self.route_request(request).await,
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
    async fn handle_request_isolated(&self, request: Request) -> Option<BackgroundExec> {
        use futures::FutureExt;
        use std::panic::AssertUnwindSafe;

        let id = request.id.clone();
        tracing::debug!(method = %request.method, id = %id, "Handling request");

        // Task-augmented `tools/call`: reply with `CreateTaskResult` now and hand
        // the tool execution back to the run loop to run in the background.
        match self.try_begin_task(&request).await {
            TaskBegin::Deferred(exec) => return Some(*exec),
            TaskBegin::Rejected => return None,
            TaskBegin::NotApplicable => {}
        }

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
        None
    }

    /// Inspect a request for task augmentation. For a task-augmented `tools/call`
    /// on a tool that supports it, create the task, reply with `CreateTaskResult`
    /// immediately, and return the background execution; otherwise leave it to the
    /// normal request path.
    async fn try_begin_task(&self, request: &Request) -> TaskBegin {
        if request.method.as_ref() != "tools/call" {
            return TaskBegin::NotApplicable;
        }
        let params = request.params.as_ref();
        let Some(task_meta) = params.and_then(|p| p.get("task")) else {
            return TaskBegin::NotApplicable;
        };
        if task_meta.is_null() {
            return TaskBegin::NotApplicable;
        }
        // Before initialization, let the normal path emit the not-initialized error.
        if !self.state.is_initialized() {
            return TaskBegin::NotApplicable;
        }
        let Some(name) = params
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            // Malformed call; let the normal path report it.
            return TaskBegin::NotApplicable;
        };
        let args = params
            .and_then(|p| p.get("arguments"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let ttl = task_meta.get("ttl").and_then(serde_json::Value::as_u64);

        let client_caps = self.state.client_caps();
        let protocol_version = self
            .state
            .protocol_version()
            .unwrap_or(ProtocolVersion::LATEST);

        // Gate on the tool's declared task support (spec: a `forbidden` tool must
        // not be task-augmented).
        let support = {
            let peer = TransportPeer::with_outbound(
                self.transport.clone(),
                self.state.clone(),
                self.config.outbound_request_timeout,
            );
            let ctx = Context::new(
                &request.id,
                None,
                &client_caps,
                &self.state.server_caps,
                protocol_version,
                &peer,
            );
            self.server.tool_task_support(&name, &ctx).await
        };
        if support == mcpkit_core::types::TaskSupport::Forbidden {
            let err = McpError::invalid_params(
                "tools/call",
                format!("tool '{name}' does not support task-augmented execution"),
            );
            let _ = self
                .transport
                .send(Message::Response(Response::error(
                    request.id.clone(),
                    err.into(),
                )))
                .await;
            return TaskBegin::Rejected;
        }

        // Create the task and reply with `CreateTaskResult` immediately.
        let handle = self.task_store.create(ttl);
        let task = handle
            .task()
            .unwrap_or_else(|| mcpkit_core::types::Task::new(handle.id().clone()));
        let create_result =
            serde_json::to_value(mcpkit_core::types::CreateTaskResult { task }).unwrap_or_default();
        if let Err(e) = self
            .transport
            .send(Message::Response(Response::success(
                request.id.clone(),
                create_result,
            )))
            .await
        {
            let err: McpError = e.into();
            tracing::error!(error = %err, "Failed to send CreateTaskResult");
        }

        let cancel = handle.cancel_token().unwrap_or_else(CancellationToken::new);
        let ctx_data = ContextData::new(
            request.id.clone(),
            client_caps,
            self.state.server_caps.clone(),
            protocol_version,
        );
        TaskBegin::Deferred(Box::new(BackgroundExec {
            handle,
            name,
            args,
            ctx_data,
            cancel,
        }))
    }

    /// Run a task-augmented tool to completion in the background, storing the
    /// result (or failure) on the task.
    async fn run_task(&self, exec: BackgroundExec) {
        let BackgroundExec {
            handle,
            name,
            args,
            ctx_data,
            cancel,
        } = exec;
        let peer = TransportPeer::with_outbound(
            self.transport.clone(),
            self.state.clone(),
            self.config.outbound_request_timeout,
        );
        let ctx = Context::with_cancellation(
            &ctx_data.request_id,
            None,
            &ctx_data.client_caps,
            &ctx_data.server_caps,
            ctx_data.protocol_version,
            &peer,
            cancel,
        );
        match self.server.call_tool_json(&name, args, &ctx).await {
            Ok(payload) => {
                let _ = handle.complete(payload);
            }
            Err(e) => {
                let _ = handle.fail(e.to_string());
            }
        }
    }

    /// Serve task queries from the built-in task store. Returns `None` for
    /// non-task methods, and for `tasks/get`/`tasks/result`/`tasks/cancel` whose
    /// id the store does not own (so a custom `with_tasks` handler can serve it).
    fn route_runtime_tasks(
        &self,
        method: &str,
        params: Option<&serde_json::Value>,
    ) -> Option<Result<serde_json::Value, McpError>> {
        let task_id = || {
            params
                .and_then(|p| p.get("taskId"))
                .and_then(|v| v.as_str())
                .map(mcpkit_core::types::TaskId::new)
        };
        match method {
            "tasks/list" => Some(Ok(serde_json::json!({ "tasks": self.task_store.list() }))),
            "tasks/get" => {
                let Some(id) = task_id() else {
                    return Some(Err(McpError::invalid_params("tasks/get", "missing taskId")));
                };
                self.task_store
                    .get(&id)
                    .map(|s| Ok(serde_json::to_value(s.task).unwrap_or_default()))
            }
            "tasks/result" => {
                let Some(id) = task_id() else {
                    return Some(Err(McpError::invalid_params(
                        "tasks/result",
                        "missing taskId",
                    )));
                };
                if let Some(payload) = self.task_store.payload(&id) {
                    Some(Ok(payload))
                } else if self.task_store.get(&id).is_some() {
                    Some(Err(McpError::invalid_params(
                        "tasks/result",
                        "task is not completed",
                    )))
                } else {
                    None
                }
            }
            "tasks/cancel" => {
                let Some(id) = task_id() else {
                    return Some(Err(McpError::invalid_params(
                        "tasks/cancel",
                        "missing taskId",
                    )));
                };
                if self.task_store.get(&id).is_some() {
                    let _ = self.task_store.cancel(&id);
                    Some(Ok(self
                        .task_store
                        .get(&id)
                        .map(|s| serde_json::to_value(s.task).unwrap_or_default())
                        .unwrap_or_default()))
                } else {
                    None
                }
            }
            _ => None,
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

        // Serve task queries from the built-in store first (falling through to a
        // custom `with_tasks` handler for ids the store does not own).
        if let Some(result) = self.route_runtime_tasks(method, params) {
            return result;
        }

        // Extract progress token from params._meta.progressToken if present
        let progress_token = extract_progress_token(params);

        // Create context for the handler. The peer is request-capable so handlers
        // can make server-initiated requests (e.g. elicitation) via `ctx.request`.
        let peer = TransportPeer::with_outbound(
            self.transport.clone(),
            self.state.clone(),
            self.config.outbound_request_timeout,
        );
        let client_caps = self.state.client_caps();
        let protocol_version = self
            .state
            .protocol_version()
            .unwrap_or(ProtocolVersion::LATEST);

        // Register a cancellation token for this request so a matching
        // `notifications/cancelled` trips the handler's `ctx.cancel`. The token
        // is removed once the handler returns.
        let cancel = CancellationToken::new();
        let cancel_key = request.id.to_string();
        self.state
            .register_cancellation(&cancel_key, cancel.clone());

        let ctx = Context::with_cancellation(
            &request.id,
            progress_token.as_ref(),
            &client_caps,
            &self.state.server_caps,
            protocol_version,
            &peer,
            cancel,
        );

        // Delegate to the router, then drop the cancellation registration.
        let result = self.server.route(method, params, &ctx).await;
        self.state.remove_cancellation(&cancel_key);
        result
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
                if let Some(request_id) = notification
                    .params
                    .as_ref()
                    .and_then(|p| p.get("requestId"))
                    .and_then(|v| serde_json::from_value::<RequestId>(v.clone()).ok())
                {
                    // Match the canonical id form `route_request` registers with,
                    // so numeric and string request ids both resolve.
                    self.state.cancel_request(&request_id.to_string());
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
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
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
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
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

    /// The task-augmentation support a tool declares (`Tool.execution.taskSupport`),
    /// used to gate task-augmented `tools/call`. Defaults to `Forbidden`.
    async fn tool_task_support(
        &self,
        _name: &str,
        _ctx: &Context<'_>,
    ) -> mcpkit_core::types::TaskSupport {
        mcpkit_core::types::TaskSupport::Forbidden
    }

    /// Run a tool to completion for task-augmented execution, returning its
    /// `CallToolResult` as JSON (the `tasks/result` payload). Defaults to
    /// method-not-found.
    async fn call_tool_json(
        &self,
        name: &str,
        _args: serde_json::Value,
        _ctx: &Context<'_>,
    ) -> Result<serde_json::Value, McpError> {
        Err(McpError::method_not_found(name))
    }
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
// Request routing
// ============================================================================

/// Single [`RequestRouter`] implementation over the typestate handler slots.
///
/// Each capability is a slot (`Registered<H>` / `NotRegistered`) exposing an
/// optional object-safe handler; routing checks each in turn. Adding a
/// dispatched capability is one slot plus one arm here -- there is no
/// per-combination explosion. The shared per-method routing logic lives in
/// [`crate::router`].
impl<H, T, R, P, K> RequestRouter for Server<H, T, R, P, K>
where
    H: ServerHandler + Send + Sync,
    T: ToolSlot,
    R: ResourceSlot,
    P: PromptSlot,
    K: TaskSlot,
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
        let page_size = self.list_page_size;
        if let Some(handler) = self.tools.as_tool_handler() {
            if let Some(result) = route_tools(handler, method, params, ctx, page_size).await {
                return result;
            }
        }
        if let Some(handler) = self.resources.as_resource_handler() {
            if let Some(result) = route_resources(handler, method, params, ctx, page_size).await {
                return result;
            }
        }
        if let Some(handler) = self.prompts.as_prompt_handler() {
            if let Some(result) = route_prompts(handler, method, params, ctx, page_size).await {
                return result;
            }
        }
        if let Some(handler) = self.tasks.as_task_handler() {
            if let Some(result) = route_tasks(handler, method, params, ctx).await {
                return result;
            }
        }
        Err(McpError::method_not_found(method))
    }

    async fn tool_task_support(
        &self,
        name: &str,
        ctx: &Context<'_>,
    ) -> mcpkit_core::types::TaskSupport {
        use mcpkit_core::types::TaskSupport;
        let Some(handler) = self.tools.as_tool_handler() else {
            return TaskSupport::Forbidden;
        };
        let tools = handler.list_tools(ctx).await.unwrap_or_default();
        tools
            .iter()
            .find(|t| t.name == name)
            .and_then(|t| t.execution.as_ref())
            .and_then(|e| e.task_support)
            .unwrap_or(TaskSupport::Forbidden)
    }

    async fn call_tool_json(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &Context<'_>,
    ) -> Result<serde_json::Value, McpError> {
        let Some(handler) = self.tools.as_tool_handler() else {
            return Err(McpError::method_not_found(name));
        };
        let output = handler.call_tool(name, args, ctx).await?;
        let result: mcpkit_core::types::CallToolResult = output.into();
        Ok(serde_json::to_value(result).unwrap_or_default())
    }
}

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

    use mcpkit_core::capability::{ClientCapabilities, ServerInfo};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::types::content::{Content, Role};
    use mcpkit_core::types::elicitation::ElicitRequest;
    use mcpkit_core::types::sampling::{CreateMessageRequest, CreateMessageResult};
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

    /// A router that answers `ping` and nothing else (like the macro-generated
    /// router's ping handling), for testing pre-initialize behavior.
    struct PingRouter;

    impl RequestRouter for PingRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("ping-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            _ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "ping" => Ok(serde_json::json!({})),
                other => Err(McpError::method_not_found(other)),
            }
        }
    }

    /// A router whose handler parks on `ctx.cancelled()` and reports whether the
    /// request was cancelled, for testing that `notifications/cancelled` trips
    /// the in-flight handler's context.
    struct CancelRouter {
        started: Arc<Notify>,
    }

    impl RequestRouter for CancelRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("cancel-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "wait_cancel" => {
                    self.started.notify_one();
                    ctx.cancelled().await;
                    Ok(serde_json::json!(ctx.is_cancelled()))
                }
                other => Err(McpError::method_not_found(other)),
            }
        }
    }

    /// A router whose `ask` handler makes a server-initiated request back to the
    /// client (`ask/upstream`) and returns its result, for testing the reverse
    /// request/response path.
    struct OutboundRouter;

    impl RequestRouter for OutboundRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("outbound-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "ask" => ctx.request("ask/upstream", None).await,
                other => Err(McpError::method_not_found(other)),
            }
        }
    }

    /// A router whose `ask_name` handler elicits a name from the user via the
    /// client and reports the outcome.
    struct ElicitRouter;

    impl RequestRouter for ElicitRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("elicit-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "ask_name" => {
                    let result = ctx
                        .elicit(ElicitRequest::text("Your name?", "name"))
                        .await?;
                    Ok(serde_json::json!({
                        "accepted": result.is_accepted(),
                        "name": result.get_string("name"),
                    }))
                }
                other => Err(McpError::method_not_found(other)),
            }
        }
    }

    /// A router whose `summarize` handler asks the client to run an LLM
    /// completion (sampling) and returns the generated text.
    struct SampleRouter;

    impl RequestRouter for SampleRouter {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("sample-test", "0.0.0")
        }
        async fn route(
            &self,
            method: &str,
            _params: Option<&serde_json::Value>,
            ctx: &Context<'_>,
        ) -> Result<serde_json::Value, McpError> {
            match method {
                "summarize" => {
                    let result = ctx
                        .create_message(CreateMessageRequest::simple("hello", 100))
                        .await?;
                    Ok(serde_json::json!({ "text": result.as_text() }))
                }
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
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
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
    async fn ping_is_answered_before_initialize() {
        let (client, server) = MemoryTransport::pair();
        // Deliberately NOT initialized: the server is mid-handshake.
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        let runtime = ServerRuntime {
            server: PingRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        // `ping` must be answered even before `initialize`.
        client.send(req("ping", 1)).await.expect("send");
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert!(
            resp.error.is_none(),
            "ping before initialize must not error: {:?}",
            resp.error
        );
        assert!(resp.result.is_some(), "ping should return a result");

        // ...but other requests are still rejected until initialized.
        client.send(req("tools/list", 2)).await.expect("send");
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(2));
        assert!(
            resp.error.is_some(),
            "non-ping requests before initialize must still be rejected"
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
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
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
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig {
                auto_initialized: true,
                max_concurrent_requests: 1,
                ..RuntimeConfig::default()
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

    #[tokio::test]
    async fn cancelled_notification_trips_in_flight_handler() {
        let (client, server) = MemoryTransport::pair();
        let started = Arc::new(Notify::new());
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        let runtime = ServerRuntime {
            server: CancelRouter {
                started: started.clone(),
            },
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        // Start a request whose handler parks on `ctx.cancelled()`.
        client.send(req("wait_cancel", 1)).await.expect("send");
        timeout(Duration::from_secs(2), started.notified())
            .await
            .expect("handler never started");

        // Cancel it by id. Before the fix this never reached the handler's token,
        // so the handler would park forever and `next_response` would time out.
        let cancel = Message::Notification(Notification::with_params(
            "notifications/cancelled".to_string(),
            serde_json::json!({ "requestId": 1 }),
        ));
        client.send(cancel).await.expect("send cancel");

        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert_eq!(
            resp.result,
            Some(serde_json::json!(true)),
            "ctx.is_cancelled() should be true after notifications/cancelled"
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn notifier_sends_list_changed_outside_request() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        let runtime = ServerRuntime {
            server: PingRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };

        // The notifier works without an active request and without running the
        // message loop — it sends straight over the shared transport.
        let notifier = runtime.notifier();
        notifier.tools_list_changed().await.expect("notify");

        let msg = timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("no notification (timed out)")
            .expect("recv ok")
            .expect("some message");
        match msg {
            Message::Notification(n) => {
                assert_eq!(n.method.as_ref(), "notifications/tools/list_changed");
            }
            other => panic!("expected a notification, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn server_initiated_request_roundtrips_at_concurrency_limit() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        let runtime = ServerRuntime {
            server: OutboundRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            // max=1: the handler holds the only slot while parked on its outbound
            // request, so the loop MUST keep receiving to route the response.
            // The old "drain at max" loop would deadlock here.
            config: RuntimeConfig {
                auto_initialized: true,
                max_concurrent_requests: 1,
                ..RuntimeConfig::default()
            },
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        // Trigger a handler that issues a server-initiated request.
        client.send(req("ask", 1)).await.expect("send");

        // The server sends us (the client) its outbound request.
        let outbound = match timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("no outbound request (timed out)")
            .expect("recv ok")
            .expect("some message")
        {
            Message::Request(r) => r,
            other => panic!("expected a server-initiated request, got {other:?}"),
        };
        assert_eq!(outbound.method.as_ref(), "ask/upstream");

        // Reply to it; the handler should resume and return the result.
        client
            .send(Message::Response(Response::success(
                outbound.id.clone(),
                serde_json::json!({ "answer": 42 }),
            )))
            .await
            .expect("send response");

        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert_eq!(resp.result, Some(serde_json::json!({ "answer": 42 })));

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn server_initiated_request_times_out() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        let runtime = ServerRuntime {
            server: OutboundRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig {
                outbound_request_timeout: Duration::from_millis(100),
                ..RuntimeConfig::default()
            },
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("ask", 1)).await.expect("send");

        // Receive the outbound request but never answer it.
        let _outbound = timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("no outbound request")
            .expect("recv ok")
            .expect("some message");

        // The handler's request times out, so its own response is an error.
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert!(resp.error.is_some(), "timed-out request should error");

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn ctx_elicit_roundtrips() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        state.set_client_caps(ClientCapabilities::default().with_elicitation());
        let runtime = ServerRuntime {
            server: ElicitRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("ask_name", 1)).await.expect("send");

        // The server sends an `elicitation/create` request to the client.
        let elicit = match timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("no elicitation request")
            .expect("recv ok")
            .expect("some message")
        {
            Message::Request(r) => r,
            other => panic!("expected elicitation/create, got {other:?}"),
        };
        assert_eq!(elicit.method.as_ref(), "elicitation/create");
        assert!(
            elicit
                .params
                .as_ref()
                .and_then(|p| p.get("requestedSchema"))
                .is_some(),
            "elicitation request should carry a requestedSchema"
        );

        // Reply as the user accepting with a name.
        client
            .send(Message::Response(Response::success(
                elicit.id.clone(),
                serde_json::json!({ "action": "accept", "content": { "name": "Ada" } }),
            )))
            .await
            .expect("send response");

        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert_eq!(
            resp.result,
            Some(serde_json::json!({ "accepted": true, "name": "Ada" }))
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn ctx_elicit_requires_client_capability() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        // The client did NOT declare the elicitation capability.
        let runtime = ServerRuntime {
            server: ElicitRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("ask_name", 1)).await.expect("send");

        // No `elicitation/create` is sent; the handler errors straight away.
        // `next_response` panics on anything other than a Response, so reaching
        // an error response proves nothing was elicited.
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert!(
            resp.error.is_some(),
            "elicit without client capability should error"
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn ctx_create_message_roundtrips() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        state.set_client_caps(ClientCapabilities::default().with_sampling());
        let runtime = ServerRuntime {
            server: SampleRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("summarize", 1)).await.expect("send");

        let sampling = match timeout(Duration::from_secs(2), client.recv())
            .await
            .expect("no sampling request")
            .expect("recv ok")
            .expect("some message")
        {
            Message::Request(r) => r,
            other => panic!("expected sampling/createMessage, got {other:?}"),
        };
        assert_eq!(sampling.method.as_ref(), "sampling/createMessage");

        // Reply as the client with a generated message.
        let result = CreateMessageResult {
            role: Role::Assistant,
            content: Content::text("a summary"),
            model: "test-model".to_string(),
            stop_reason: None,
        };
        client
            .send(Message::Response(Response::success(
                sampling.id.clone(),
                serde_json::to_value(result).expect("serialize result"),
            )))
            .await
            .expect("send response");

        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert_eq!(
            resp.result,
            Some(serde_json::json!({ "text": "a summary" }))
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn ctx_create_message_requires_client_capability() {
        let (client, server) = MemoryTransport::pair();
        let state = Arc::new(ServerState::new(ServerCapabilities::default()));
        state.set_initialized();
        // The client did NOT declare the sampling capability.
        let runtime = ServerRuntime {
            server: SampleRouter,
            transport: Arc::new(server),
            state,
            task_store: Arc::new(crate::capability::tasks::TaskManager::new()),
            config: RuntimeConfig::default(),
        };
        let handle = tokio::spawn(async move { runtime.run().await });

        client.send(req("summarize", 1)).await.expect("send");

        // No `sampling/createMessage` is sent; the handler errors immediately.
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert!(
            resp.error.is_some(),
            "create_message without client capability should error"
        );

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

    #[tokio::test]
    async fn task_augmented_tools_call_runs_in_background() {
        use crate::builder::ServerBuilder;
        use crate::handler::{ServerHandler, ToolHandler};
        use mcpkit_core::protocol::Request;
        use mcpkit_core::types::{TaskSupport, Tool, ToolOutput};

        struct H;
        impl ServerHandler for H {
            fn server_info(&self) -> ServerInfo {
                ServerInfo::new("t", "1.0.0")
            }
        }
        impl ToolHandler for H {
            async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
                Ok(vec![Tool::new("slow").task_support(TaskSupport::Optional)])
            }
            async fn call_tool(
                &self,
                name: &str,
                _args: serde_json::Value,
                _ctx: &Context<'_>,
            ) -> Result<ToolOutput, McpError> {
                Ok(ToolOutput::text(format!("done:{name}")))
            }
        }

        let request = |id: u64, method: &'static str, params: serde_json::Value| {
            Message::Request(Request {
                jsonrpc: "2.0".into(),
                id: RequestId::Number(id),
                method: method.into(),
                params: Some(params),
            })
        };

        let (client, server_tr) = MemoryTransport::pair();
        let built = ServerBuilder::new(H).with_tools(H).build();
        let runtime = ServerRuntime::new(built, server_tr);
        runtime.state().set_initialized();
        let handle = tokio::spawn(async move { runtime.run().await });

        // A task-augmented tools/call returns CreateTaskResult immediately.
        client
            .send(request(
                1,
                "tools/call",
                serde_json::json!({ "name": "slow", "arguments": {}, "task": {} }),
            ))
            .await
            .expect("send");
        let resp = next_response(&client).await;
        assert_eq!(resp.id, RequestId::Number(1));
        assert!(
            resp.error.is_none(),
            "augmented call errored: {:?}",
            resp.error
        );
        let result = resp.result.expect("create result");
        assert_eq!(result["task"]["status"], "working");
        let task_id = result["task"]["taskId"]
            .as_str()
            .expect("taskId")
            .to_string();

        // The tool runs in the background; tasks/result yields its payload once done.
        let mut payload = None;
        for attempt in 0..100u64 {
            client
                .send(request(
                    100 + attempt,
                    "tasks/result",
                    serde_json::json!({ "taskId": task_id }),
                ))
                .await
                .expect("send");
            let r = next_response(&client).await;
            if r.error.is_none() {
                payload = r.result;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let payload = payload.expect("task completed with a payload");
        assert!(
            payload["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("done:slow"),
            "unexpected task payload: {payload}"
        );

        // tasks/get reports the terminal status.
        client
            .send(request(
                999,
                "tasks/get",
                serde_json::json!({ "taskId": task_id }),
            ))
            .await
            .expect("send");
        let got = next_response(&client).await;
        assert_eq!(got.result.expect("task")["status"], "completed");

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn task_augmented_call_on_forbidden_tool_is_rejected() {
        use crate::builder::ServerBuilder;
        use crate::handler::{ServerHandler, ToolHandler};
        use mcpkit_core::protocol::Request;
        use mcpkit_core::types::{Tool, ToolOutput};

        struct H;
        impl ServerHandler for H {
            fn server_info(&self) -> ServerInfo {
                ServerInfo::new("t", "1.0.0")
            }
        }
        impl ToolHandler for H {
            async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
                // No execution.taskSupport -> forbidden by default.
                Ok(vec![Tool::new("plain")])
            }
            async fn call_tool(
                &self,
                _name: &str,
                _args: serde_json::Value,
                _ctx: &Context<'_>,
            ) -> Result<ToolOutput, McpError> {
                Ok(ToolOutput::text("ok"))
            }
        }

        let (client, server_tr) = MemoryTransport::pair();
        let runtime = ServerRuntime::new(ServerBuilder::new(H).with_tools(H).build(), server_tr);
        runtime.state().set_initialized();
        let handle = tokio::spawn(async move { runtime.run().await });

        client
            .send(Message::Request(Request {
                jsonrpc: "2.0".into(),
                id: RequestId::Number(1),
                method: "tools/call".into(),
                params: Some(serde_json::json!({ "name": "plain", "task": {} })),
            }))
            .await
            .expect("send");
        let resp = next_response(&client).await;
        assert!(
            resp.error.is_some(),
            "a forbidden tool must reject task augmentation"
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }

    /// The validation decorator must also cover the *task* path: a
    /// task-augmented `tools/call` whose arguments violate the `inputSchema`
    /// must resolve to an `isError` result rather than running the tool body.
    /// This exercises `Server::call_tool_json` (background execution), a
    /// different call site than `route_tools`.
    #[cfg(feature = "schema-validation")]
    #[tokio::test]
    async fn task_path_validates_input_via_decorator() {
        use crate::builder::ServerBuilder;
        use crate::handler::{ServerHandler, ToolHandler};
        use mcpkit_core::protocol::Request;
        use mcpkit_core::types::{TaskSupport, Tool, ToolOutput};

        struct H;
        impl ServerHandler for H {
            fn server_info(&self) -> ServerInfo {
                ServerInfo::new("t", "1.0.0")
            }
        }
        impl ToolHandler for H {
            async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
                Ok(vec![
                    Tool::new("slow")
                        .task_support(TaskSupport::Optional)
                        .input_schema(serde_json::json!({
                            "type": "object",
                            "properties": { "n": { "type": "number" } },
                            "required": ["n"]
                        })),
                ])
            }
            async fn call_tool(
                &self,
                name: &str,
                _args: serde_json::Value,
                _ctx: &Context<'_>,
            ) -> Result<ToolOutput, McpError> {
                Ok(ToolOutput::text(format!("done:{name}")))
            }
        }

        let request = |id: u64, method: &'static str, params: serde_json::Value| {
            Message::Request(Request {
                jsonrpc: "2.0".into(),
                id: RequestId::Number(id),
                method: method.into(),
                params: Some(params),
            })
        };

        let (client, server_tr) = MemoryTransport::pair();
        // `validate_tool_io()` wraps the tool handler; background task execution
        // must still route through it.
        let built = ServerBuilder::new(H)
            .with_tools(H)
            .validate_tool_io()
            .build();
        let runtime = ServerRuntime::new(built, server_tr);
        runtime.state().set_initialized();
        let handle = tokio::spawn(async move { runtime.run().await });

        // Task-augmented call with input that violates the schema (missing "n").
        client
            .send(request(
                1,
                "tools/call",
                serde_json::json!({ "name": "slow", "arguments": {}, "task": {} }),
            ))
            .await
            .expect("send");
        let resp = next_response(&client).await;
        let task_id = resp.result.expect("create result")["task"]["taskId"]
            .as_str()
            .expect("taskId")
            .to_string();

        let mut payload = None;
        for attempt in 0..100u64 {
            client
                .send(request(
                    100 + attempt,
                    "tasks/result",
                    serde_json::json!({ "taskId": task_id }),
                ))
                .await
                .expect("send");
            let r = next_response(&client).await;
            if r.error.is_none() {
                payload = r.result;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let payload = payload.expect("task completed with a payload");
        assert_eq!(
            payload["isError"],
            serde_json::json!(true),
            "task path must validate input: {payload}"
        );
        assert!(
            !payload["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("done:slow"),
            "the tool body must not have run: {payload}"
        );

        drop(client);
        let _ = timeout(Duration::from_secs(2), handle).await;
    }
}
