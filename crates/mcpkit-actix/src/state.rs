//! Shared state for MCP Actix handlers.

use crate::session::{SessionManager, SessionStore};
use mcpkit_core::auth::ProtectedResourceMetadata;
use mcpkit_core::capability::ServerInfo;
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
    /// Session manager for SSE streaming connections.
    pub sse_sessions: Arc<SessionManager>,
    /// Server info for the initialize response.
    pub server_info: ServerInfo,
    /// Validates request `Origin` headers (DNS-rebinding protection). Defaults
    /// to loopback-only.
    pub origin_validator: Arc<OriginValidator>,
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
            sse_sessions: Arc::new(SessionManager::new()),
            server_info,
            origin_validator: Arc::new(OriginValidator::default()),
            list_page_size: None,
            completion: None,
        }
    }

    /// Create new MCP state with custom session configuration.
    pub fn with_sessions(handler: H, sessions: SessionStore, sse_sessions: SessionManager) -> Self {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            server_info,
            sessions: Arc::new(sessions),
            sse_sessions: Arc::new(sse_sessions),
            origin_validator: Arc::new(OriginValidator::default()),
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
            sse_sessions: Arc::clone(&self.sse_sessions),
            server_info: self.server_info.clone(),
            origin_validator: Arc::clone(&self.origin_validator),
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
}
