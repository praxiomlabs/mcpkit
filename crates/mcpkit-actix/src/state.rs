//! Shared state for MCP Actix handlers.

use crate::session::{SessionManager, SessionStore};
use mcpkit_core::auth::ProtectedResourceMetadata;
use mcpkit_core::capability::ServerInfo;
use mcpkit_server::ServerHandler;
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
#[derive(Debug)]
pub struct McpState<H> {
    /// The user's MCP handler.
    pub handler: Arc<H>,
    /// Session store for tracking HTTP sessions.
    pub sessions: Arc<SessionStore>,
    /// Session manager for SSE streaming connections.
    pub sse_sessions: Arc<SessionManager>,
    /// Server info for the initialize response.
    pub server_info: ServerInfo,
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
