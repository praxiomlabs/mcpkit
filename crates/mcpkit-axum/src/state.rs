//! Shared state for MCP Axum handlers.

use crate::session::{SessionManager, SessionStore};
use mcpkit_core::capability::ServerInfo;
use std::sync::Arc;

/// Shared state for MCP handlers.
///
/// This struct holds all the shared state needed by MCP HTTP handlers,
/// including session management and server information.
#[derive(Clone)]
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
