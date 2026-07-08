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
use mcpkit_core::types::roots::{ListRootsResult, Root};
use mcpkit_core::types::sampling::{CreateMessageRequest, CreateMessageResult};
use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;

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

// The cancellation token is shared with the client-side task machinery and
// lives in `mcpkit_core::tasks`; re-exported here for path stability.
pub use mcpkit_core::tasks::{CancellationToken, CancelledFuture};

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

/// Sentinel [`RequestId`] for notification-scoped contexts (see
/// [`Context::for_notification`]). Notifications have no request id; this is not
/// a real JSON-RPC id.
static NOTIFICATION_REQUEST_ID: std::sync::LazyLock<RequestId> =
    std::sync::LazyLock::new(|| RequestId::String("__notification__".to_string()));

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

    /// Create a context for handling an inbound client notification.
    ///
    /// Notifications carry no JSON-RPC request id, so
    /// [`request_id`](Self::request_id) is a documented sentinel
    /// (`__notification__`) that must **not** be treated as a real id. The
    /// context is still outbound-capable: a hook may call
    /// [`list_roots`](Self::list_roots) or send notifications, and those
    /// server-to-client requests allocate their own ids via the peer.
    #[must_use]
    pub fn for_notification(
        client_caps: &'a ClientCapabilities,
        server_caps: &'a ServerCapabilities,
        protocol_version: ProtocolVersion,
        peer: &'a dyn Peer,
    ) -> Self {
        Self {
            request_id: &NOTIFICATION_REQUEST_ID,
            progress_token: None,
            client_caps,
            server_caps,
            protocol_version,
            peer,
            cancel: CancellationToken::new(),
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

    /// Request the roots this client exposes (`roots/list`).
    ///
    /// Requires the client to have declared the `roots` capability.
    ///
    /// # Errors
    ///
    /// Returns an error if the client did not declare roots support, or the
    /// request fails, times out, or the response could not be parsed.
    pub async fn list_roots(&self) -> Result<Vec<Root>, McpError> {
        if !self.client_caps.has_roots() {
            return Err(McpError::internal(
                "the client did not declare the roots capability",
            ));
        }
        let result = self.request("roots/list", None).await?;
        let result: ListRootsResult = serde_json::from_value(result).map_err(McpError::from)?;
        Ok(result.roots)
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

    #[tokio::test]
    async fn list_roots_requests_and_parses_when_advertised() {
        use mcpkit_core::protocol::Response;

        struct RootsPeer;
        impl Peer for RootsPeer {
            fn notify(
                &self,
                _n: Notification,
            ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>> {
                Box::pin(async { Ok(()) })
            }
            fn request(
                &self,
                method: Cow<'static, str>,
                _params: Option<serde_json::Value>,
            ) -> Pin<Box<dyn Future<Output = Result<Response, McpError>> + Send + '_>> {
                assert_eq!(method, "roots/list");
                let result = serde_json::to_value(ListRootsResult {
                    roots: vec![Root::new("file:///a").name("a")],
                    meta: None,
                })
                .unwrap();
                Box::pin(async move { Ok(Response::success(RequestId::Number(1), result)) })
            }
        }

        let request_id = RequestId::Number(1);
        let server_caps = ServerCapabilities::default();
        let peer = RootsPeer;

        // Advertised -> request sent and result parsed.
        let client_caps = ClientCapabilities::default().with_roots();
        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );
        let roots = ctx.list_roots().await.expect("roots listed");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name.as_deref(), Some("a"));

        // Not advertised -> error before any request.
        let no_roots = ClientCapabilities::default();
        let ctx = Context::new(
            &request_id,
            None,
            &no_roots,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );
        assert!(ctx.list_roots().await.is_err());
    }
}
