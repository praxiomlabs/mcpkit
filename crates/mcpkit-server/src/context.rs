//! Request context for MCP handlers.
//!
//! The context provides access to the current request state and allows
//! handlers to interact with the connection (sending notifications,
//! progress updates, etc.).
//!
//! # Key Features
//!
//! - **Borrowing-friendly**: Uses lifetime references, NO `'static` requirement
//! - **Progress reporting**: Send progress updates for long-running operations
//! - **Cancellation**: Check if the request has been cancelled
//! - **Notifications**: Send notifications back to the client via Peer trait
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::{Context, NoOpPeer, ContextData};
//! use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
//! use mcpkit_core::protocol::RequestId;
//! use mcpkit_core::protocol_version::ProtocolVersion;
//!
//! // Create test context data
//! let data = ContextData::new(
//!     RequestId::Number(1),
//!     ClientCapabilities::default(),
//!     ServerCapabilities::default(),
//!     ProtocolVersion::LATEST,
//! );
//! let peer = NoOpPeer;
//!
//! // Create a context from the data
//! let ctx = Context::new(
//!     &data.request_id,
//!     data.progress_token.as_ref(),
//!     &data.client_caps,
//!     &data.server_caps,
//!     data.protocol_version,
//!     &peer,
//! );
//!
//! // Check for cancellation and protocol version
//! assert!(!ctx.is_cancelled());
//! assert!(ctx.protocol_version.supports_tasks());
//! ```

use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::{Notification, ProgressToken, RequestId, Response};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_core::types::elicitation::{ElicitRequest, ElicitResult, UrlElicitRequest};
use mcpkit_core::types::logging::{LoggingLevel, LoggingMessageNotificationParams};
use mcpkit_core::types::notifications::ProgressNotificationParams;
use mcpkit_core::types::sampling::{CreateMessageRequest, CreateMessageResult};
use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context as TaskContext, Poll};

use event_listener::Event;

/// Trait for sending messages to the peer (client or server).
///
/// This trait abstracts over the transport layer, allowing the context
/// to send notifications without knowing the underlying transport.
pub trait Peer: Send + Sync {
    /// Send a notification to the peer.
    fn notify(
        &self,
        notification: Notification,
    ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>>;

    /// Send a request to the peer and await its response.
    ///
    /// Used for server-initiated requests such as elicitation and sampling. The
    /// implementation assigns the request id and correlates the response.
    ///
    /// The default implementation returns an error: a peer that has no
    /// persistent bidirectional connection (for example a one-shot HTTP
    /// response) cannot make server-initiated requests.
    fn request(
        &self,
        method: Cow<'static, str>,
        params: Option<serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Response, McpError>> + Send + '_>> {
        let _ = (method, params);
        Box::pin(async {
            Err(McpError::internal(
                "this peer does not support server-initiated requests",
            ))
        })
    }
}

/// A cancellation token for tracking request cancellation.
///
/// Wraps an atomic flag plus an [`event_listener::Event`] so waiters can park
/// until cancellation instead of busy-polling the flag.
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    event: Arc<Event>,
}

impl CancellationToken {
    /// Create a new cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            event: Arc::new(Event::new()),
        }
    }

    /// Check if cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // Wake every task currently waiting in `cancelled()`.
        self.event.notify(usize::MAX);
    }

    /// Wait for cancellation.
    ///
    /// Returns a future that completes when cancellation is requested. The
    /// future parks on an [`event_listener::Event`] and is woken by
    /// [`cancel`](Self::cancel); it does not busy-poll.
    #[must_use]
    pub fn cancelled(&self) -> CancelledFuture {
        CancelledFuture::new(self.cancelled.clone(), self.event.clone())
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A future that completes when cancellation is requested.
///
/// Parks on the token's [`event_listener::Event`] until cancellation, rather
/// than waking itself on every poll.
pub struct CancelledFuture {
    inner: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl CancelledFuture {
    fn new(cancelled: Arc<AtomicBool>, event: Arc<Event>) -> Self {
        Self {
            inner: Box::pin(async move {
                loop {
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    // Register a listener *before* the final flag check so a
                    // `cancel()` that races with us cannot be missed: if it set
                    // the flag after our first check, the re-check below catches
                    // it; if it fires after, the listener is woken.
                    let listener = event.listen();
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    listener.await;
                }
            }),
        }
    }
}

impl Future for CancelledFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

/// Request context passed to handler methods.
///
/// The context uses lifetime references to avoid `'static` requirements
/// and Arc overhead. This enables:
/// - Single-threaded async without Arc overhead
/// - `!Send` types in handlers (important for some runtimes)
/// - Users who need spawning can wrap in Arc themselves
///
/// Per the plan: "Request context - passed by reference, NO 'static requirement"
pub struct Context<'a> {
    /// The request ID for this operation.
    pub request_id: &'a RequestId,
    /// Optional progress token for reporting progress.
    pub progress_token: Option<&'a ProgressToken>,
    /// Client capabilities negotiated during initialization.
    pub client_caps: &'a ClientCapabilities,
    /// Server capabilities advertised during initialization.
    pub server_caps: &'a ServerCapabilities,
    /// The negotiated protocol version.
    ///
    /// Use this to check version-specific feature availability via
    /// methods like `supports_tasks()`, `supports_elicitation()`, etc.
    pub protocol_version: ProtocolVersion,
    /// Peer for sending notifications.
    peer: &'a dyn Peer,
    /// Cancellation token for this request.
    cancel: CancellationToken,
}

impl<'a> Context<'a> {
    /// Create a new context with all required references.
    #[must_use]
    pub fn new(
        request_id: &'a RequestId,
        progress_token: Option<&'a ProgressToken>,
        client_caps: &'a ClientCapabilities,
        server_caps: &'a ServerCapabilities,
        protocol_version: ProtocolVersion,
        peer: &'a dyn Peer,
    ) -> Self {
        Self {
            request_id,
            progress_token,
            client_caps,
            server_caps,
            protocol_version,
            peer,
            cancel: CancellationToken::new(),
        }
    }

    /// Create a new context with a custom cancellation token.
    #[must_use]
    pub fn with_cancellation(
        request_id: &'a RequestId,
        progress_token: Option<&'a ProgressToken>,
        client_caps: &'a ClientCapabilities,
        server_caps: &'a ServerCapabilities,
        protocol_version: ProtocolVersion,
        peer: &'a dyn Peer,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            request_id,
            progress_token,
            client_caps,
            server_caps,
            protocol_version,
            peer,
            cancel,
        }
    }

    /// Check if the request has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Get a future that completes when the request is cancelled.
    pub fn cancelled(&self) -> impl Future<Output = ()> + '_ {
        self.cancel.cancelled()
    }

    /// Get the cancellation token for this context.
    #[must_use]
    pub const fn cancellation_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Send a notification to the client.
    ///
    /// # Arguments
    ///
    /// * `method` - The notification method name
    /// * `params` - Optional notification parameters
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn notify(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), McpError> {
        let notification = if let Some(p) = params {
            Notification::with_params(method.to_string(), p)
        } else {
            Notification::new(method.to_string())
        };
        self.peer.notify(notification).await
    }

    /// Report progress for this operation.
    ///
    /// This sends a progress notification to the client if a progress token
    /// was provided with the request.
    ///
    /// # Arguments
    ///
    /// * `current` - Current progress value
    /// * `total` - Total progress value (if known)
    /// * `message` - Optional progress message
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn progress(
        &self,
        current: f64,
        total: Option<f64>,
        message: Option<&str>,
    ) -> Result<(), McpError> {
        let Some(token) = self.progress_token else {
            // No progress token, silently succeed
            return Ok(());
        };

        let params = ProgressNotificationParams {
            total,
            message: message.map(String::from),
            ..ProgressNotificationParams::new(token.clone(), current)
        };

        self.notify(
            "notifications/progress",
            Some(serde_json::to_value(params)?),
        )
        .await
    }

    /// Emit a `notifications/message` log to the client at `level`, optionally
    /// tagged with a `logger` name and carrying arbitrary JSON `data`.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification could not be sent.
    pub async fn log(
        &self,
        level: LoggingLevel,
        logger: Option<&str>,
        data: serde_json::Value,
    ) -> Result<(), McpError> {
        let params = LoggingMessageNotificationParams {
            logger: logger.map(String::from),
            ..LoggingMessageNotificationParams::new(level, data)
        };
        self.notify("notifications/message", Some(serde_json::to_value(params)?))
            .await
    }

    /// Send a request to the client and await its response.
    ///
    /// This is the basis for server-initiated requests (e.g. elicitation,
    /// sampling). The peer assigns the request id and correlates the response;
    /// the request is aborted if this context is cancelled.
    ///
    /// # Errors
    ///
    /// Returns an error if the request was cancelled, the peer does not support
    /// requests, the request timed out, or the response carried a JSON-RPC
    /// error.
    pub async fn request(
        &self,
        method: impl Into<Cow<'static, str>>,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, McpError> {
        use futures::future::{Either, select};

        let request = self.peer.request(method.into(), params);
        let cancelled = self.cancel.cancelled();
        let response = match select(request, cancelled).await {
            Either::Left((result, _)) => result?,
            Either::Right(((), _)) => return Err(McpError::internal("request cancelled")),
        };

        if let Some(error) = response.error {
            return Err(McpError::internal(error.message));
        }
        response
            .result
            .ok_or_else(|| McpError::internal("response contained neither result nor error"))
    }

    /// Request structured input from the user through the client (form-mode
    /// elicitation).
    ///
    /// Sends an `elicitation/create` request and awaits the user's response
    /// (accept with content, decline, or cancel). This requires the client to
    /// have declared the `elicitation` capability and the negotiated protocol
    /// version to support elicitation.
    ///
    /// # Errors
    ///
    /// Returns an error if the client did not declare elicitation support, the
    /// negotiated protocol version predates elicitation, the request was
    /// cancelled or timed out, or the response could not be parsed.
    pub async fn elicit(&self, request: ElicitRequest) -> Result<ElicitResult, McpError> {
        if !self.protocol_version.supports_elicitation() {
            return Err(McpError::internal(
                "the negotiated protocol version does not support elicitation",
            ));
        }
        if !self.client_caps.has_elicitation() {
            return Err(McpError::internal(
                "the client did not declare the elicitation capability",
            ));
        }

        let params = serde_json::to_value(&request).map_err(McpError::from)?;
        let result = self.request("elicitation/create", Some(params)).await?;
        serde_json::from_value(result).map_err(McpError::from)
    }

    /// Request a URL-mode elicitation: ask the client to have the user navigate
    /// to a URL for an out-of-band interaction (e.g. authorization or payment).
    ///
    /// Returns the client's [`ElicitResult`] action once the user consents to
    /// open the URL. When the out-of-band interaction later finishes, notify the
    /// client with `ServerNotifier::elicitation_complete(elicitation_id)`.
    ///
    /// Gated on the client's `elicitation.url` sub-capability (which is only
    /// declared on 2025-11-25+).
    ///
    /// # Security
    ///
    /// Per the MCP spec, the caller MUST use an unguessable `elicitation_id`
    /// bound to a verified user identity and MUST NOT place credentials in the
    /// URL. mcpkit provides the mechanism; associating the id with a user is the
    /// application's responsibility (see the session-binding helpers, #86).
    ///
    /// # Errors
    ///
    /// Returns an error if the negotiated protocol version does not support
    /// elicitation, the client did not declare URL-mode elicitation, or the
    /// request fails.
    pub async fn elicit_url(&self, request: UrlElicitRequest) -> Result<ElicitResult, McpError> {
        if !self.protocol_version.supports_elicitation() {
            return Err(McpError::internal(
                "the negotiated protocol version does not support elicitation",
            ));
        }
        if !self.client_caps.has_url_elicitation() {
            return Err(McpError::internal(
                "the client did not declare URL-mode elicitation support",
            ));
        }

        let params = serde_json::to_value(&request).map_err(McpError::from)?;
        let result = self.request("elicitation/create", Some(params)).await?;
        serde_json::from_value(result).map_err(McpError::from)
    }

    /// Request the client to run an LLM completion (sampling).
    ///
    /// Sends a `sampling/createMessage` request and awaits the generated
    /// message. This requires the client to have declared the `sampling`
    /// capability (sampling is available in every protocol version).
    ///
    /// # Errors
    ///
    /// Returns an error if the client did not declare sampling support, the
    /// request was cancelled or timed out, or the response could not be parsed.
    pub async fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> Result<CreateMessageResult, McpError> {
        if !self.client_caps.has_sampling() {
            return Err(McpError::internal(
                "the client did not declare the sampling capability",
            ));
        }

        let params = serde_json::to_value(&request).map_err(McpError::from)?;
        let result = self.request("sampling/createMessage", Some(params)).await?;
        serde_json::from_value(result).map_err(McpError::from)
    }
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("request_id", &self.request_id)
            .field("progress_token", &self.progress_token)
            .field("client_caps", &self.client_caps)
            .field("server_caps", &self.server_caps)
            .field("protocol_version", &self.protocol_version)
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// A no-op peer implementation for testing.
///
/// This peer silently accepts all notifications without sending them anywhere.
#[derive(Debug, Clone, Copy)]
pub struct NoOpPeer;

impl Peer for NoOpPeer {
    fn notify(
        &self,
        _notification: Notification,
    ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

/// Owned data for creating contexts.
///
/// This struct holds owned copies of all the data needed to create a Context.
/// It's useful when you need to create contexts from owned data.
pub struct ContextData {
    /// The request ID.
    pub request_id: RequestId,
    /// Optional progress token.
    pub progress_token: Option<ProgressToken>,
    /// Client capabilities.
    pub client_caps: ClientCapabilities,
    /// Server capabilities.
    pub server_caps: ServerCapabilities,
    /// The negotiated protocol version.
    pub protocol_version: ProtocolVersion,
}

impl ContextData {
    /// Create a new context data struct.
    #[must_use]
    pub const fn new(
        request_id: RequestId,
        client_caps: ClientCapabilities,
        server_caps: ServerCapabilities,
        protocol_version: ProtocolVersion,
    ) -> Self {
        Self {
            request_id,
            progress_token: None,
            client_caps,
            server_caps,
            protocol_version,
        }
    }

    /// Set the progress token.
    #[must_use]
    pub fn with_progress_token(mut self, token: ProgressToken) -> Self {
        self.progress_token = Some(token);
        self
    }

    /// Create a context from this data with the given peer.
    #[must_use]
    pub fn to_context<'a>(&'a self, peer: &'a dyn Peer) -> Context<'a> {
        Context::new(
            &self.request_id,
            self.progress_token.as_ref(),
            &self.client_caps,
            &self.server_caps,
            self.protocol_version,
            peer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    /// Regression test for #8: `cancelled()` must park on a waker instead of
    /// busy-spinning (the old impl called `wake_by_ref()` on every poll). We
    /// poll with a waker that counts wake-ups and assert the future does not
    /// wake itself, then that `cancel()` wakes it and it resolves.
    #[test]
    fn cancelled_future_parks_and_wakes_on_cancel() {
        use std::sync::atomic::AtomicUsize;
        use std::task::{Wake, Waker};

        struct CountingWaker(AtomicUsize);
        impl Wake for CountingWaker {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut cx = TaskContext::from_waker(&waker);

        let token = CancellationToken::new();
        let mut fut = Box::pin(token.cancelled());

        // First poll: not cancelled -> must be Pending and must NOT have woken
        // itself (a busy-spin would wake immediately).
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            0,
            "cancelled future must park, not busy-spin (no self-wake)"
        );

        // Cancelling wakes the registered waker and the future resolves.
        token.cancel();
        assert!(
            counter.0.load(Ordering::SeqCst) >= 1,
            "cancel() must wake the parked waiter"
        );
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    /// A token already cancelled before `cancelled()` is awaited resolves
    /// immediately.
    #[test]
    fn cancelled_future_ready_when_already_cancelled() {
        let waker = std::task::Waker::noop();
        let mut cx = TaskContext::from_waker(waker);

        let token = CancellationToken::new();
        token.cancel();
        let mut fut = Box::pin(token.cancelled());
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    #[test]
    fn test_context_creation() {
        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;

        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );

        assert!(!ctx.is_cancelled());
        assert!(ctx.progress_token.is_none());
        assert_eq!(ctx.protocol_version, ProtocolVersion::LATEST);
    }

    #[test]
    fn test_context_with_progress_token() {
        let request_id = RequestId::Number(1);
        let progress_token = ProgressToken::String("token".to_string());
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;

        let ctx = Context::new(
            &request_id,
            Some(&progress_token),
            &client_caps,
            &server_caps,
            ProtocolVersion::V2025_03_26,
            &peer,
        );

        assert!(ctx.progress_token.is_some());
        assert_eq!(ctx.protocol_version, ProtocolVersion::V2025_03_26);
    }

    #[test]
    fn test_context_data() {
        let data = ContextData::new(
            RequestId::Number(42),
            ClientCapabilities::default(),
            ServerCapabilities::default(),
            ProtocolVersion::V2025_06_18,
        )
        .with_progress_token(ProgressToken::String("test".to_string()));

        let peer = NoOpPeer;
        let ctx = data.to_context(&peer);

        assert!(ctx.progress_token.is_some());
        assert_eq!(ctx.protocol_version, ProtocolVersion::V2025_06_18);
        // Test feature detection via protocol version
        assert!(ctx.protocol_version.supports_elicitation());
        assert!(!ctx.protocol_version.supports_tasks()); // Tasks require 2025-11-25
    }
}
