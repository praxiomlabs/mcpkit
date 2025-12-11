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
use mcpkit_core::protocol::{Notification, ProgressToken, RequestId};
use mcpkit_core::protocol_version::ProtocolVersion;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
}

/// A cancellation token for tracking request cancellation.
///
/// This is a simple wrapper around an atomic boolean that can be
/// shared across threads and checked for cancellation.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a new cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
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
    }

    /// Wait for cancellation.
    ///
    /// Returns a future that completes when cancellation is requested.
    ///
    /// Note: In a production implementation, this would integrate with the
    /// runtime's notification system. This simple implementation polls
    /// the atomic flag.
    #[must_use]
    pub fn cancelled(&self) -> CancelledFuture {
        CancelledFuture {
            cancelled: self.cancelled.clone(),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A future that completes when cancellation is requested.
pub struct CancelledFuture {
    cancelled: Arc<AtomicBool>,
}

impl Future for CancelledFuture {
    type Output = ();

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.cancelled.load(Ordering::SeqCst) {
            std::task::Poll::Ready(())
        } else {
            // Wake up later to check again
            // In production, this would register with a proper notification system
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
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
        current: u64,
        total: Option<u64>,
        message: Option<&str>,
    ) -> Result<(), McpError> {
        let Some(token) = self.progress_token else {
            // No progress token, silently succeed
            return Ok(());
        };

        let params = serde_json::json!({
            "progressToken": token,
            "progress": current,
            "total": total,
            "message": message,
        });

        self.notify("notifications/progress", Some(params)).await
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
