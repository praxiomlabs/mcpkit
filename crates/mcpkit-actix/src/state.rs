//! Shared state for MCP Actix handlers.

use crate::session::SessionStore;
use mcpkit_core::auth::ProtectedResourceMetadata;
use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_server::ServerHandler;
use mcpkit_transport::http::OriginValidator;
use std::sync::Arc;

/// Trait for types that provide server info.
pub trait HasServerInfo {
    /// Returns the server info.
    fn server_info(&self) -> ServerInfo;
}

impl<T: ServerHandler> HasServerInfo for T {
    fn server_info(&self) -> ServerInfo {
        ServerHandler::server_info(self)
    }
}

/// Shared state for MCP Actix handlers.
///
/// This struct holds all the state needed by MCP HTTP handlers, including
/// the user's handler implementation and session management.
///
/// Note: Clone is implemented manually to avoid requiring `H: Clone`.
/// The handler is wrapped in `Arc`, so cloning only clones the Arc pointer.
pub struct McpState<H> {
    /// The user's MCP handler.
    pub handler: Arc<H>,
    /// Session store for tracking HTTP sessions.
    pub sessions: Arc<SessionStore>,
    /// Server info for the initialize response.
    pub server_info: ServerInfo,
    /// Validates request `Origin` headers (DNS-rebinding protection). Defaults
    /// to loopback-only.
    pub origin_validator: Arc<OriginValidator>,
    /// Timeouts for server-initiated (peer) requests, by method class.
    pub peer_timeouts: mcpkit_server::adapter_peer::PeerTimeouts,
    /// Reconnect grace for peer requests. Fixed by design; overridable only
    /// for tests via `McpRouter::with_reconnect_grace`.
    pub(crate) reconnect_grace: std::time::Duration,
    /// Page size for `*/list` results; `None` disables pagination.
    pub list_page_size: Option<usize>,
    /// Optional completion handler for `completion/complete`.
    pub completion: Option<Arc<dyn mcpkit_server::dispatch::DynCompletionHandler>>,
}

// Manual Debug to avoid requiring `H: Debug` and because the completion handler
// is a trait object.
impl<H> std::fmt::Debug for McpState<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpState")
            .field("handler", &format_args!("Arc<H>"))
            .field("server_info", &self.server_info)
            .field("list_page_size", &self.list_page_size)
            .finish_non_exhaustive()
    }
}

impl<H> McpState<H>
where
    H: HasServerInfo,
{
    /// Create new MCP state with the given handler.
    pub fn new(handler: H) -> Self {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            sessions: Arc::new(SessionStore::with_default_timeout()),
            server_info,
            origin_validator: Arc::new(OriginValidator::default()),
            peer_timeouts: mcpkit_server::adapter_peer::PeerTimeouts::default(),
            reconnect_grace: mcpkit_server::adapter_peer::RECONNECT_GRACE,
            list_page_size: None,
            completion: None,
        }
    }

    /// Create new MCP state with custom session configuration.
    pub fn with_sessions(handler: H, sessions: SessionStore) -> Self {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            server_info,
            sessions: Arc::new(sessions),
            origin_validator: Arc::new(OriginValidator::default()),
            peer_timeouts: mcpkit_server::adapter_peer::PeerTimeouts::default(),
            reconnect_grace: mcpkit_server::adapter_peer::RECONNECT_GRACE,
            list_page_size: None,
            completion: None,
        }
    }
}

impl<H> Clone for McpState<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            sessions: Arc::clone(&self.sessions),
            server_info: self.server_info.clone(),
            origin_validator: Arc::clone(&self.origin_validator),
            peer_timeouts: self.peer_timeouts,
            reconnect_grace: self.reconnect_grace,
            list_page_size: self.list_page_size,
            completion: self.completion.clone(),
        }
    }
}

/// State for OAuth discovery endpoints.
///
/// This struct holds the OAuth 2.1 Protected Resource Metadata (RFC 9728)
/// that is served at `.well-known/oauth-protected-resource`.
#[derive(Clone, Debug)]
pub struct OAuthState {
    /// Protected resource metadata per RFC 9728.
    pub metadata: ProtectedResourceMetadata,
}

impl OAuthState {
    /// Create new OAuth state with the given metadata.
    #[must_use]
    pub const fn new(metadata: ProtectedResourceMetadata) -> Self {
        Self { metadata }
    }
}

impl<H> McpState<H> {
    /// Enable pagination of `*/list` results at the given page size.
    ///
    /// By default pagination is disabled (lists return everything with no
    /// `nextCursor`). A size of `0` is treated as disabled.
    #[must_use]
    pub const fn with_list_page_size(mut self, page_size: usize) -> Self {
        self.list_page_size = Some(page_size);
        self
    }

    /// Register a completion handler so this adapter answers
    /// `completion/complete`.
    #[must_use]
    pub fn with_completion<C: mcpkit_server::CompletionHandler + 'static>(
        mut self,
        completion: C,
    ) -> Self {
        self.completion = Some(Arc::new(completion));
        self
    }

    /// Set the default task retention (milliseconds) for each session's task
    /// store, applied when a task-augmented `tools/call` omits a `ttl`. Pass
    /// `None` for unlimited retention. Defaults to
    /// [`DEFAULT_TASK_TTL_MS`](mcpkit_server::capability::tasks::DEFAULT_TASK_TTL_MS).
    #[must_use]
    pub fn with_task_ttl(mut self, default_task_ttl: Option<u64>) -> Self {
        if let Some(store) = Arc::get_mut(&mut self.sessions) {
            store.default_task_ttl = default_task_ttl;
        }
        self
    }
}

impl<H: mcpkit_server::ServerHandler> McpState<H> {
    /// The handler's advertised capabilities, plus `completions` when a
    /// completion handler is registered on this adapter (the handler itself
    /// cannot know it was registered here, so the adapter advertises it).
    #[must_use]
    pub fn effective_capabilities(&self) -> ServerCapabilities {
        let caps = self.handler.capabilities();
        if self.completion.is_some() && !caps.has_completions() {
            caps.with_completions()
        } else {
            caps
        }
    }
}
