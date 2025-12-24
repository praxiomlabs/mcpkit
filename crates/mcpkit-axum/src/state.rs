//! Shared state for MCP Axum handlers.

use crate::session::{SessionManager, SessionStore};
use mcpkit_core::auth::ProtectedResourceMetadata;
use mcpkit_core::capability::ServerInfo;
use std::fmt;
use std::sync::Arc;

/// Shared state for MCP handlers.
///
/// This struct holds all the shared state needed by MCP HTTP handlers,
/// including session management and server information.
///
/// Note: Clone and Debug are implemented manually to avoid requiring `H: Clone` or `H: Debug`.
/// The handler is wrapped in `Arc`, so cloning only clones the Arc pointer.
pub struct McpState<H> {
    /// The MCP server handler.
    pub handler: Arc<H>,
    /// Server information for initialization responses.
    pub server_info: ServerInfo,
    /// Session store for HTTP request tracking.
    pub sessions: Arc<SessionStore>,
    /// Session manager for SSE streaming.
    pub sse_sessions: Arc<SessionManager>,
}

// Manual Clone implementation to avoid requiring H: Clone
impl<H> Clone for McpState<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            server_info: self.server_info.clone(),
            sessions: Arc::clone(&self.sessions),
            sse_sessions: Arc::clone(&self.sse_sessions),
        }
    }
}

// Manual Debug implementation to avoid requiring H: Debug
impl<H> fmt::Debug for McpState<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpState")
            .field("handler", &format_args!("Arc<H>"))
            .field("server_info", &self.server_info)
            .field("sessions", &self.sessions)
            .field("sse_sessions", &format_args!("Arc<SessionManager>"))
            .finish()
    }
}

impl<H> McpState<H> {
    /// Create new MCP state with a handler.
    pub fn new(handler: H) -> Self
    where
        H: HasServerInfo,
    {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            server_info,
            sessions: Arc::new(SessionStore::with_default_timeout()),
            sse_sessions: Arc::new(SessionManager::new()),
        }
    }

    /// Create new MCP state with custom session configuration.
    pub fn with_sessions(handler: H, sessions: SessionStore, sse_sessions: SessionManager) -> Self
    where
        H: HasServerInfo,
    {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            server_info,
            sessions: Arc::new(sessions),
            sse_sessions: Arc::new(sse_sessions),
        }
    }
}

/// Trait for handlers that can provide server info.
pub trait HasServerInfo {
    /// Get the server information.
    fn server_info(&self) -> ServerInfo;
}

// Blanket implementation for types that implement ServerHandler
impl<T> HasServerInfo for T
where
    T: mcpkit_server::ServerHandler,
{
    fn server_info(&self) -> ServerInfo {
        <T as mcpkit_server::ServerHandler>::server_info(self)
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
