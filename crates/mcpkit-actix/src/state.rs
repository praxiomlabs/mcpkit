//! Shared state for MCP Actix handlers.

use crate::session::{SessionManager, SessionStore};
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
pub struct McpConfig<H> {
    /// The user's MCP handler.
    pub handler: Arc<H>,
    /// Session manager for tracking HTTP sessions.
    pub sessions: SessionManager,
    /// SSE session store for streaming connections.
    pub sse_sessions: SessionStore,
    /// Server info for the initialize response.
    pub server_info: ServerInfo,
}

impl<H> McpConfig<H>
where
    H: HasServerInfo,
{
    /// Create new MCP config with the given handler.
    pub fn new(handler: H) -> Self {
        let server_info = handler.server_info();
        Self {
            handler: Arc::new(handler),
            sessions: SessionManager::new(),
            sse_sessions: SessionStore::new(),
            server_info,
        }
    }
}

impl<H> Clone for McpConfig<H> {
    fn clone(&self) -> Self {
        Self {
            handler: Arc::clone(&self.handler),
            sessions: self.sessions.clone(),
            sse_sessions: self.sse_sessions.clone(),
            server_info: self.server_info.clone(),
        }
    }
}
